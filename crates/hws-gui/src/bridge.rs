//! The only bridge between QML and the engine.
//!
//! QML sees one singleton, `Backend`, with:
//!
//! * `stateJson` — the whole application state, re-emitted after every change.
//! * `run(command, payloadJson)` — perform a change.
//! * `query(what, payloadJson)` — read something that is not part of the state,
//!   such as the generated Lua preview.
//! * `notify(message, isError)` — a signal for the toast.
//!
//! Keeping the surface this small means the UI holds no configuration logic,
//! and the engine stays testable without Qt.

/// The CXX-Qt bridge definition.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// An alias to the QString type.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The singleton QML talks to.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, state_json, cxx_name = "stateJson")]
        #[qproperty(bool, ready)]
        #[qproperty(QString, shot_dir, cxx_name = "shotDir")]
        type Backend = super::BackendRust;
    }

    extern "RustQt" {
        /// Emitted after a command, for the status toast.
        #[qsignal]
        #[cxx_name = "notify"]
        fn notify(self: Pin<&mut Backend>, message: &QString, is_error: bool);
    }

    extern "RustQt" {
        /// Run a command. `payload` is a JSON object, or empty for none.
        #[qinvokable]
        #[cxx_name = "run"]
        fn run(self: Pin<&mut Backend>, command: &QString, payload: &QString);

        /// Read something that is not part of `stateJson`.
        ///
        /// `what` is one of `preview`, `importPreview`, `shaderSource`,
        /// `state`. Returns JSON, or plain text for `preview` and
        /// `shaderSource`. An error comes back as `{"error": "..."}`.
        #[qinvokable]
        #[cxx_name = "query"]
        fn query(self: &Backend, what: &QString, payload: &QString) -> QString;

        /// Re-read the shader directory and the live compositor state.
        #[qinvokable]
        #[cxx_name = "refresh"]
        fn refresh(self: Pin<&mut Backend>);
    }

    impl cxx_qt::Initialize for Backend {}
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use hws_core::Session;
use serde_json::{json, Value};

/// The Rust side of the singleton.
#[derive(Default)]
pub struct BackendRust {
    state_json: QString,
    /// Set from HWS_SHOT_DIR. When non-empty the UI renders each page to a PNG
    /// there and quits — a development aid for checking layout without a
    /// compositor, and the only thing in this file that is not about the config.
    shot_dir: QString,
    ready: bool,
    session: Option<Session>,
}

impl cxx_qt::Initialize for qobject::Backend {
    /// Load everything once the QObject exists.
    fn initialize(mut self: Pin<&mut Self>) {
        let shot_dir = std::env::var("HWS_SHOT_DIR").unwrap_or_default();
        self.as_mut().set_shot_dir(QString::from(&shot_dir));

        let session = Session::load();
        let problems = session.problems.clone();
        self.as_mut().rust_mut().session = Some(session);
        self.as_mut().push_state();
        self.as_mut().set_ready(true);

        for problem in problems {
            let msg = QString::from(&problem);
            self.as_mut().notify(&msg, true);
        }
    }
}

impl qobject::Backend {
    /// Run a command and push the new state.
    pub fn run(mut self: Pin<&mut Self>, command: &QString, payload: &QString) {
        let command = command.to_string();
        let payload = parse_payload(&payload.to_string());

        let outcome = match self.as_mut().rust_mut().session.as_mut() {
            Some(session) => session.command(&command, &payload),
            None => Err(hws_core::Error::other("the backend is still starting up")),
        };

        self.as_mut().push_state();

        match outcome {
            Ok(Some(message)) => {
                let m = QString::from(&message);
                self.as_mut().notify(&m, false);
            }
            Ok(None) => {}
            Err(e) => {
                let m = QString::from(&e.to_string());
                self.as_mut().notify(&m, true);
            }
        }
    }

    /// Answer a read-only question.
    pub fn query(&self, what: &QString, payload: &QString) -> QString {
        let Some(session) = self.rust().session.as_ref() else {
            return QString::from("{}");
        };
        let what = what.to_string();
        let payload = parse_payload(&payload.to_string());

        let answer: Result<String, String> = match what.as_str() {
            "state" => Ok(session.state().to_string()),
            "preview" => session.preview().map_err(|e| e.to_string()),
            "importPreview" => {
                session.import_preview().map(|v| v.to_string()).map_err(|e| e.to_string())
            }
            "shaderSource" => payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "no shader path given".to_string())
                .and_then(|p| {
                    hws_core::paths::read_to_string(&hws_core::paths::expand(p))
                        .map_err(|e| e.to_string())
                }),
            other => Err(format!("unknown query `{other}`")),
        };

        match answer {
            Ok(text) => QString::from(&text),
            Err(e) => QString::from(&json!({ "error": e }).to_string()),
        }
    }

    /// Re-read the shader directory and the compositor state.
    pub fn refresh(mut self: Pin<&mut Self>) {
        if let Some(session) = self.as_mut().rust_mut().session.as_mut() {
            session.rescan_shaders();
            session.refresh_live();
        }
        self.push_state();
    }

    /// Serialise the session and hand it to QML.
    fn push_state(mut self: Pin<&mut Self>) {
        let json = match self.as_ref().rust().session.as_ref() {
            Some(session) => session.state().to_string(),
            None => "{}".to_string(),
        };
        self.as_mut().set_state_json(QString::from(&json));
    }
}

/// Turn a payload string into a JSON object, tolerating an empty string.
fn parse_payload(text: &str) -> Value {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return json!({});
    }
    serde_json::from_str(trimmed).unwrap_or_else(|_| json!({}))
}
