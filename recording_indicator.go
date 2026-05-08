package main

import (
	"image/color"
	"math"
	"sync"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
)

// 录音指示器：无边框居中悬浮窗，黑色圆角底 + 实时音量驱动的横向波形条
const (
	indicatorBarCount  = 40
	indicatorBarWidth  = 4
	indicatorBarGap    = 4
	indicatorBarMaxH   = 60
	indicatorBarMinH   = 4
	indicatorWindowW   = 520
	indicatorWindowH   = 140
	indicatorRefreshMs = 33 // 30fps
	// 音量历史环形缓冲，每个 bar 位对应一个历史帧，产生从右向左滚动的波形
	indicatorSmooth = 0.6 // 平滑系数：当前帧与上一帧的插值权重
)

var (
	indicatorMu     sync.Mutex
	indicatorWindow fyne.Window
	indicatorStop   chan struct{}
)

// ShowRecordingIndicator 显示录音指示器悬浮窗。
// levelFn 返回当前音量（0.0-1.0），由动画循环轮询。
// 必须从非主 goroutine 调用（内部通过 fyne.Do 调度到主线程）。
func ShowRecordingIndicator(app fyne.App, levelFn func() float32) {
	if _, ok := app.(desktop.App); !ok {
		return // 无托盘支持的平台直接跳过
	}

	indicatorMu.Lock()
	if indicatorWindow != nil {
		indicatorMu.Unlock()
		return // 已在显示
	}
	stop := make(chan struct{})
	indicatorStop = stop
	indicatorMu.Unlock()

	fyne.DoAndWait(func() {
		drv, ok := fyne.CurrentApp().Driver().(desktop.Driver)
		if !ok {
			return
		}
		w := drv.CreateSplashWindow()
		w.Resize(fyne.NewSize(indicatorWindowW, indicatorWindowH))
		w.SetPadded(false)
		w.SetFixedSize(true)

		// 黑色圆角背景（作为"伪毛玻璃"的深色底板）
		bg := canvas.NewRectangle(color.NRGBA{R: 12, G: 12, B: 18, A: 230})
		bg.CornerRadius = 20

		// 波形条
		bars := make([]*canvas.Rectangle, indicatorBarCount)
		barContainer := container.NewWithoutLayout()
		totalW := float32(indicatorBarCount*(indicatorBarWidth+indicatorBarGap) - indicatorBarGap)
		startX := (float32(indicatorWindowW) - totalW) / 2
		centerY := float32(indicatorWindowH)/2 - 20
		for i := range bars {
			bar := canvas.NewRectangle(indicatorBarColor(0))
			bar.CornerRadius = 2
			bar.Resize(fyne.NewSize(indicatorBarWidth, indicatorBarMinH))
			bar.Move(fyne.NewPos(
				startX+float32(i)*(indicatorBarWidth+indicatorBarGap),
				centerY-indicatorBarMinH/2,
			))
			bars[i] = bar
			barContainer.Add(bar)
		}

		// 底部文字
		label := canvas.NewText("🎙 正在录音…  再按热键结束", color.NRGBA{R: 200, G: 200, B: 220, A: 255})
		label.TextSize = 13
		label.Alignment = fyne.TextAlignCenter
		label.Move(fyne.NewPos(0, float32(indicatorWindowH)-34))
		label.Resize(fyne.NewSize(indicatorWindowW, 20))

		content := container.NewWithoutLayout(bg, barContainer, label)
		bg.Resize(fyne.NewSize(indicatorWindowW, indicatorWindowH))
		barContainer.Resize(fyne.NewSize(indicatorWindowW, indicatorWindowH))

		w.SetContent(content)
		w.CenterOnScreen()
		w.Show()

		indicatorMu.Lock()
		indicatorWindow = w
		indicatorMu.Unlock()

		go runIndicatorAnimation(bars, barContainer, levelFn, stop)
	})
}

// HideRecordingIndicator 关闭悬浮窗，可从任意 goroutine 调用。
func HideRecordingIndicator() {
	indicatorMu.Lock()
	w := indicatorWindow
	stop := indicatorStop
	indicatorWindow = nil
	indicatorStop = nil
	indicatorMu.Unlock()

	if stop != nil {
		close(stop)
	}
	if w != nil {
		fyne.Do(func() {
			w.Close()
		})
	}
}

// runIndicatorAnimation 30fps 刷新波形条高度：
// 每帧把最新音量写入历史环形缓冲，bars 按缓冲值渲染，实现从右向左滚动。
func runIndicatorAnimation(
	bars []*canvas.Rectangle,
	barContainer *fyne.Container,
	levelFn func() float32,
	stop <-chan struct{},
) {
	history := make([]float32, indicatorBarCount)
	prev := float32(0)
	ticker := time.NewTicker(indicatorRefreshMs * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-stop:
			return
		case <-ticker.C:
			// 平滑当前音量
			raw := levelFn()
			smoothed := prev*indicatorSmooth + raw*(1-indicatorSmooth)
			prev = smoothed

			// 左移历史，把最新值放到最右侧
			copy(history, history[1:])
			history[len(history)-1] = smoothed

			fyne.Do(func() {
				updateBarHeights(bars, history)
				barContainer.Refresh()
			})
		}
	}
}

func updateBarHeights(bars []*canvas.Rectangle, history []float32) {
	centerY := float32(indicatorWindowH)/2 - 20
	for i, bar := range bars {
		level := history[i]
		// 非线性曲线：小音量也能有感知高度（sqrt）
		h := float32(math.Sqrt(float64(level))) * indicatorBarMaxH
		if h < indicatorBarMinH {
			h = indicatorBarMinH
		}
		bar.Resize(fyne.NewSize(indicatorBarWidth, h))
		bar.Move(fyne.NewPos(
			bar.Position().X,
			centerY-h/2,
		))
		bar.FillColor = indicatorBarColor(level)
	}
}

// indicatorBarColor 根据音量返回颜色：低音量偏蓝，高音量偏品红色，增强视觉反馈。
func indicatorBarColor(level float32) color.Color {
	// 冷色 (100, 180, 255) → 暖色 (255, 80, 180)
	lo := level
	if lo > 1 {
		lo = 1
	}
	r := uint8(100 + (255-100)*lo)
	g := uint8(180 + (80-180)*lo)
	b := uint8(255 + (180-255)*lo)
	return color.NRGBA{R: r, G: g, B: b, A: 230}
}
