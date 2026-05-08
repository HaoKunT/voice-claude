.PHONY: help build install uninstall build-win \
        rust-dev rust-build rust-install rust-build-win \
        rust-test rust-clippy rust-fmt rust-fmt-check \
        go-build go-install go-build-win go-lint go-test go-vuln \
        clean

APP_NAME    = voice-claude
APP_BUNDLE  = $(APP_NAME).app
BINARY      = $(APP_NAME)
BINARY_WIN  = $(APP_NAME).exe
RUST_DIR    = rust

help:
	@echo "voice-claude 构建命令："
	@echo ""
	@echo "  [Rust + Tauri 版 —— 主线]"
	@echo "    make rust-dev         开发模式跑 Tauri（热重载）"
	@echo "    make rust-build       macOS 打包 .app + .dmg"
	@echo "    make rust-install     macOS 编译 + 安装到 /Applications"
	@echo "    make rust-build-win   Windows 打包 .exe + .msi"
	@echo "    make rust-test        跑 Rust 测试 + 前端 typecheck"
	@echo "    make rust-clippy      Rust clippy lint"
	@echo "    make rust-fmt         格式化 Rust 代码"
	@echo ""
	@echo "  [Go + Fyne 旧版 —— 归档，保留到 Rust 版稳定]"
	@echo "    make go-build         macOS 打包旧 .app"
	@echo "    make go-install       macOS 编译 + 安装旧 .app"
	@echo "    make go-build-win     Windows 交叉编译 Go 版"
	@echo "    make go-test / lint / vuln"
	@echo ""
	@echo "  [通用]"
	@echo "    make uninstall / clean"
	@echo ""
	@echo "默认目标（build / install / build-win）指向 Rust 版。"

# ── 默认目标（Rust 版）────────────────────────────────────────────────────

build: rust-build
install: rust-install
build-win: rust-build-win

# ── Rust + Tauri ─────────────────────────────────────────────────────────

rust-dev:
	cd $(RUST_DIR) && pnpm install && pnpm tauri dev

rust-build:
	cd $(RUST_DIR) && pnpm install && pnpm tauri build --bundles app,dmg
	@echo "✓ Rust 版构建完成: $(RUST_DIR)/src-tauri/target/release/bundle/"

rust-install: rust-build
	@BUNDLE=$$(find $(RUST_DIR)/src-tauri/target/release/bundle/macos -maxdepth 1 -name "*.app" | head -n1); \
	if [ -z "$$BUNDLE" ]; then echo "未找到 .app bundle" && exit 1; fi; \
	rm -rf "/Applications/$(APP_BUNDLE)"; \
	cp -r "$$BUNDLE" "/Applications/$(APP_BUNDLE)"; \
	rm -rf "$$BUNDLE"; \
	codesign --force --deep --sign - \
		--identifier com.haokunt.voice-claude \
		--entitlements $(RUST_DIR)/src-tauri/entitlements.plist \
		"/Applications/$(APP_BUNDLE)" >/dev/null 2>&1 && \
		echo "✓ 签名已固定为 com.haokunt.voice-claude + entitlements 已嵌入" || \
		echo "⚠ 重签失败（可忽略）"; \
	echo "✓ 已安装到 /Applications/$(APP_BUNDLE)"

rust-build-win:
	cd $(RUST_DIR) && pnpm install && pnpm tauri build --target x86_64-pc-windows-msvc --bundles msi,nsis

rust-test:
	cd $(RUST_DIR) && pnpm install --frozen-lockfile
	cd $(RUST_DIR) && pnpm typecheck
	cd $(RUST_DIR)/src-tauri && cargo test --locked

rust-clippy:
	cd $(RUST_DIR)/src-tauri && cargo clippy --all-targets -- -D warnings

rust-fmt:
	cd $(RUST_DIR)/src-tauri && cargo fmt

rust-fmt-check:
	cd $(RUST_DIR)/src-tauri && cargo fmt --check

# ── Go + Fyne 旧版（保留到 Rust 版稳定）──────────────────────────────────

go-build:
	CGO_ENABLED=1 go build -o $(BINARY) .
	rm -rf $(APP_BUNDLE)
	mkdir -p $(APP_BUNDLE)/Contents/MacOS
	mkdir -p $(APP_BUNDLE)/Contents/Resources
	cp $(BINARY) $(APP_BUNDLE)/Contents/MacOS/$(APP_NAME)
	cp icon.png $(APP_BUNDLE)/Contents/Resources/icon.png
	cp Info.plist $(APP_BUNDLE)/Contents/Info.plist
	@echo "✓ Go 版构建完成: $(APP_BUNDLE)"

go-install: go-build
	rm -rf /Applications/$(APP_BUNDLE)
	cp -r $(APP_BUNDLE) /Applications/$(APP_BUNDLE)
	rm -rf $(APP_BUNDLE) $(BINARY)
	@echo "✓ 已安装到 /Applications/$(APP_BUNDLE)"

go-build-win:
	CGO_ENABLED=1 GOOS=windows GOARCH=amd64 \
		go build -tags nosherpa -ldflags="-H windowsgui" -o $(BINARY_WIN) .

go-lint:
	golangci-lint run ./...

go-test:
	CGO_ENABLED=1 go test -tags nosherpa -race -shuffle=on -count=1 ./...

go-vuln:
	govulncheck ./...

# ── 通用 ──────────────────────────────────────────────────────────────────

uninstall:
	@if [ -d "/Applications/$(APP_BUNDLE)" ]; then \
		rm -rf "/Applications/$(APP_BUNDLE)"; \
		echo "✓ 已卸载 /Applications/$(APP_BUNDLE)"; \
	else \
		echo "未找到 /Applications/$(APP_BUNDLE)"; \
	fi

clean:
	rm -f $(BINARY) $(BINARY_WIN)
	rm -rf $(APP_BUNDLE)
	rm -rf $(RUST_DIR)/src-tauri/target $(RUST_DIR)/dist $(RUST_DIR)/node_modules
