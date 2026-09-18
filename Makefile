PREFIX ?= /usr/local
DESTDIR ?=
CARGO ?= cargo

BIN := hyprwindowshade-gui

.PHONY: all build release test check install uninstall clean shots

all: release

build:
	$(CARGO) build

release:
	$(CARGO) build --release

# The engine is where the decisions live, and it needs no display to test.
test:
	$(CARGO) test -p hws-core

check:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings -A clippy::field_reassign_with_default

install: release
	install -Dm755 target/release/$(BIN) $(DESTDIR)$(PREFIX)/bin/$(BIN)
	install -Dm644 packaging/$(BIN).desktop \
		$(DESTDIR)$(PREFIX)/share/applications/$(BIN).desktop
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(BIN)/LICENSE
	install -Dm644 README.md $(DESTDIR)$(PREFIX)/share/doc/$(BIN)/README.md

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(BIN).desktop
	rm -rf $(DESTDIR)$(PREFIX)/share/licenses/$(BIN)
	rm -rf $(DESTDIR)$(PREFIX)/share/doc/$(BIN)

# Render every page to a PNG without a compositor.
shots: build
	@mkdir -p shots
	HWS_SHOT_DIR=$(PWD)/shots QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
		./target/debug/$(BIN) || true

clean:
	$(CARGO) clean
