.PHONY: help build install uninstall build-win dev test lint fmt clean \
        legacy-build legacy-install legacy-build-win legacy-test legacy-lint legacy-vuln

APP_NAME    = voice-claude
APP_BUNDLE  = $(APP_NAME).app
RUST_DIR    = rust
LEGACY_DIR  = legacy/go

help:
	@echo "voice-claude 构建命令："
	@echo ""
	@echo "  [主线：Rust + Tauri]"
	@echo "    make dev           开发模式（Tauri 热重载）"
	@echo "    make build         macOS 打包 .app"
	@echo "    make install       macOS 编译 + 安装到 /Applications"
	@echo "    make build-win     Windows 打包 .msi + .exe"
	@echo "    make test          cargo test + 前端 typecheck"
	@echo "    make lint          clippy + fmt --check"
	@echo "    make fmt           cargo fmt + 前端格式化"
	@echo ""
	@echo "  [归档：Go + Fyne（legacy/go/）]"
	@echo "    make legacy-build   / legacy-install / legacy-build-win"
	@echo "    make legacy-test / legacy-lint / legacy-vuln"
	@echo ""
	@echo "  [通用]"
	@echo "    make uninstall / clean"

# ── 默认主线目标 ────────────────────────────────────────────────────────────

dev:
	cd $(RUST_DIR) && pnpm install && pnpm tauri dev

build:
	cd $(RUST_DIR) && pnpm install && pnpm tauri build --bundles app
	@echo "✓ 构建完成: $(RUST_DIR)/src-tauri/target/release/bundle/macos/$(APP_BUNDLE)"

install: build
	@BUNDLE=$$(find $(RUST_DIR)/src-tauri/target/release/bundle/macos -maxdepth 1 -name "*.app" | head -n1); \
	if [ -z "$$BUNDLE" ]; then echo "未找到 .app bundle" && exit 1; fi; \
	rm -rf "/Applications/$(APP_BUNDLE)"; \
	cp -r "$$BUNDLE" "/Applications/$(APP_BUNDLE)"; \
	rm -rf "$$BUNDLE"; \
	codesign --force --deep --sign - \
		--identifier com.haokunt.voice-claude \
		--entitlements $(RUST_DIR)/src-tauri/entitlements.plist \
		"/Applications/$(APP_BUNDLE)" >/dev/null 2>&1 && \
		echo "✓ 签名已固定 + entitlements 已嵌入" || \
		echo "⚠ 重签失败（可忽略）"; \
	echo "✓ 已安装到 /Applications/$(APP_BUNDLE)"

build-win:
	cd $(RUST_DIR) && pnpm install && pnpm tauri build --target x86_64-pc-windows-msvc --bundles msi,nsis

test:
	cd $(RUST_DIR) && pnpm install --frozen-lockfile
	cd $(RUST_DIR) && pnpm typecheck
	cd $(RUST_DIR)/src-tauri && cargo test --locked

lint:
	cd $(RUST_DIR)/src-tauri && cargo fmt --check
	cd $(RUST_DIR)/src-tauri && cargo clippy --all-targets -- -D warnings
	cd $(RUST_DIR) && pnpm typecheck

fmt:
	cd $(RUST_DIR)/src-tauri && cargo fmt
	cd $(RUST_DIR) && pnpm typecheck

# ── Go + Fyne 归档（legacy/go/）────────────────────────────────────────────

legacy-build:
	cd $(LEGACY_DIR) && CGO_ENABLED=1 go build -o $(APP_NAME) .
	cd $(LEGACY_DIR) && rm -rf $(APP_BUNDLE) && \
		mkdir -p $(APP_BUNDLE)/Contents/MacOS $(APP_BUNDLE)/Contents/Resources && \
		cp $(APP_NAME) $(APP_BUNDLE)/Contents/MacOS/$(APP_NAME) && \
		cp ../../icon.png $(APP_BUNDLE)/Contents/Resources/icon.png && \
		cp Info.plist $(APP_BUNDLE)/Contents/Info.plist
	@echo "✓ Go 版 legacy 构建完成: $(LEGACY_DIR)/$(APP_BUNDLE)"

legacy-install: legacy-build
	rm -rf /Applications/$(APP_BUNDLE)
	cp -r $(LEGACY_DIR)/$(APP_BUNDLE) /Applications/$(APP_BUNDLE)
	rm -rf $(LEGACY_DIR)/$(APP_BUNDLE) $(LEGACY_DIR)/$(APP_NAME)
	@echo "✓ 已安装 Go 版到 /Applications/$(APP_BUNDLE)"

legacy-build-win:
	cd $(LEGACY_DIR) && CGO_ENABLED=1 GOOS=windows GOARCH=amd64 \
		go build -tags nosherpa -ldflags="-H windowsgui" -o $(APP_NAME).exe .

legacy-test:
	cd $(LEGACY_DIR) && CGO_ENABLED=1 go test -tags nosherpa -race -shuffle=on -count=1 ./...

legacy-lint:
	cd $(LEGACY_DIR) && golangci-lint run ./...

legacy-vuln:
	cd $(LEGACY_DIR) && govulncheck ./...

# ── 通用 ──────────────────────────────────────────────────────────────────

uninstall:
	@if [ -d "/Applications/$(APP_BUNDLE)" ]; then \
		rm -rf "/Applications/$(APP_BUNDLE)"; \
		echo "✓ 已卸载 /Applications/$(APP_BUNDLE)"; \
	else \
		echo "未找到 /Applications/$(APP_BUNDLE)"; \
	fi

clean:
	rm -rf $(RUST_DIR)/src-tauri/target $(RUST_DIR)/dist $(RUST_DIR)/node_modules
	rm -f $(LEGACY_DIR)/$(APP_NAME) $(LEGACY_DIR)/$(APP_NAME).exe
	rm -rf $(LEGACY_DIR)/$(APP_BUNDLE)
