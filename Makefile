.PHONY: build install uninstall build-win lint lint-fix test vuln clean

APP_NAME   = voice-claude
APP_BUNDLE = $(APP_NAME).app
BINARY     = $(APP_NAME)
BINARY_WIN = $(APP_NAME).exe

# ── macOS ──────────────────────────────────────────────────────────────────

# 编译并打包 .app（本地，带 SenseVoice）
build:
	CGO_ENABLED=1 go build -o $(BINARY) .
	rm -rf $(APP_BUNDLE)
	mkdir -p $(APP_BUNDLE)/Contents/MacOS
	mkdir -p $(APP_BUNDLE)/Contents/Resources
	cp $(BINARY) $(APP_BUNDLE)/Contents/MacOS/$(APP_NAME)
	cp icon.png $(APP_BUNDLE)/Contents/Resources/icon.png
	cp Info.plist $(APP_BUNDLE)/Contents/Info.plist
	@echo "✓ 构建完成: $(APP_BUNDLE)"

# 编译 + 打包 + 安装到 /Applications（一行搞定）
install: build
	rm -rf /Applications/$(APP_BUNDLE)
	cp -r $(APP_BUNDLE) /Applications/$(APP_BUNDLE)
	rm -rf $(APP_BUNDLE) $(BINARY)
	@echo "✓ 已安装到 /Applications/$(APP_BUNDLE)"

uninstall:
	@if [ -d "/Applications/$(APP_BUNDLE)" ]; then \
		rm -rf "/Applications/$(APP_BUNDLE)"; \
		echo "✓ 已卸载 /Applications/$(APP_BUNDLE)"; \
	else \
		echo "未找到 /Applications/$(APP_BUNDLE)"; \
	fi

# ── Windows ────────────────────────────────────────────────────────────────

build-win:
	CGO_ENABLED=1 GOOS=windows GOARCH=amd64 \
		go build -tags nosherpa -ldflags="-H windowsgui" -o $(BINARY_WIN) .

# ── 质量检查 ──────────────────────────────────────────────────────────────

lint:
	golangci-lint run ./...

lint-fix:
	golangci-lint run --fix ./...

test:
	CGO_ENABLED=1 go test -tags nosherpa -race -shuffle=on -count=1 ./...

vuln:
	govulncheck ./...

# ── 清理 ──────────────────────────────────────────────────────────────────

clean:
	rm -f $(BINARY) $(BINARY_WIN) $(APP_NAME)-amd64 $(APP_NAME)-arm64.exe
	rm -rf $(APP_BUNDLE)
