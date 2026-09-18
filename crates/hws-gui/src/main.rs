//! hyprwindowshade-gui — a Qt Quick front end for the HyprWindowShade plugin.

#![deny(missing_docs)]

pub mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

fn main() {
    // `--print-state` loads everything and dumps the state document without
    // starting Qt. Useful for checking what the app sees on a machine where the
    // GUI will not start, and for testing in CI.
    if std::env::args().any(|a| a == "--print-state") {
        let session = hws_core::Session::load();
        println!("{:#}", session.state());
        return;
    }
    if std::env::args().any(|a| a == "--print-block") {
        let session = hws_core::Session::load();
        match session.preview() {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Qt Quick Controls' Basic style is the one that honours a custom palette
    // without fighting the platform theme, which is what the Gruvbox themes
    // need. Setting it here means the app looks the same on any desktop.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    }

    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("hyprwindowshade-gui"));
        app.set_organization_name(&QString::from("hyprwindowshade"));
    }

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // Qt 6.5 added `:/qt/qml` to the default import path; on 6.4 the module's
        // own qmldir is never found there, so its QML components resolve as
        // "not a type" while its C++ types still work. Adding it explicitly is a
        // no-op on newer Qt.
        engine.as_mut().add_import_path(&QString::from("qrc:/qt/qml"));

        // Without this, a QML failure leaves an empty process with no window and
        // no explanation.
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                eprintln!(
                    "hyprwindowshade-gui: the interface failed to load ({}). \
                     Qt Quick Controls may be missing — on Arch that is the \
                     qt6-declarative package.",
                    url.to_string()
                );
                std::process::exit(1);
            })
            .release();

        let root = std::env::var("HWS_ROOT_QML")
            .unwrap_or_else(|_| "qrc:/qt/qml/dev/hyprwindowshade/gui/qml/Main.qml".to_string());
        engine.load(&QUrl::from(&root));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
