package main

import (
	"fmt"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
)

// ShowHistory 打开历史记录窗口，展示最近 200 条识别记录。
func ShowHistory(app fyne.App) {
	w := app.NewWindow("识别历史")
	w.Resize(fyne.NewSize(640, 480))

	entries, err := LoadHistory(200)
	if err != nil {
		dialog.ShowError(err, w)
		return
	}

	if len(entries) == 0 {
		w.SetContent(container.NewCenter(widget.NewLabel("暂无历史记录")))
		w.Show()
		return
	}

	list := widget.NewList(
		func() int { return len(entries) },
		func() fyne.CanvasObject {
			return container.NewVBox(
				widget.NewLabel(""),
				widget.NewLabel(""),
			)
		},
		func(i widget.ListItemID, obj fyne.CanvasObject) {
			e := entries[i]
			box := obj.(*fyne.Container)                //nolint:forcetypeassert // widget tree constructed in this function, type is known
			timeLabel := box.Objects[0].(*widget.Label) //nolint:forcetypeassert // widget tree constructed in this function, type is known
			textLabel := box.Objects[1].(*widget.Label) //nolint:forcetypeassert // widget tree constructed in this function, type is known

			timeLabel.SetText(fmt.Sprintf("%s  ·  %s", e.CreatedAt.Format("01-02 15:04:05"), e.ASRProvider))
			text := e.CorrectedText
			if text == "" {
				text = e.RawText
			}
			if len([]rune(text)) > 60 {
				text = string([]rune(text)[:60]) + "…"
			}
			textLabel.SetText(text)
		},
	)

	list.OnSelected = func(i widget.ListItemID) {
		e := entries[i]
		text := e.CorrectedText
		if text == "" {
			text = e.RawText
		}
		showEntryDetail(app, w, e, text, func() {
			// 删除后刷新
			entries = append(entries[:i], entries[i+1:]...)
			list.Refresh()
		})
	}

	clearBtn := widget.NewButton("清空全部", func() {
		dialog.ShowConfirm("确认清空", "确定要清空所有历史记录吗？", func(ok bool) {
			if !ok {
				return
			}
			if err := ClearHistory(); err != nil {
				dialog.ShowError(err, w)
				return
			}
			entries = nil
			list.Refresh()
		}, w)
	})

	w.SetContent(container.NewBorder(
		nil,
		container.NewCenter(clearBtn),
		nil, nil,
		list,
	))
	w.Show()
}

func showEntryDetail(app fyne.App, _ fyne.Window, e HistoryEntry, text string, onDelete func()) {
	d := app.NewWindow("记录详情")
	d.Resize(fyne.NewSize(480, 300))

	timeStr := e.CreatedAt.Format(time.DateTime)
	infoLabel := widget.NewLabel(fmt.Sprintf("时间：%s\n后端：%s", timeStr, e.ASRProvider))

	rawEntry := widget.NewMultiLineEntry()
	rawEntry.SetText(e.RawText)
	rawEntry.Disable()

	correctedEntry := widget.NewMultiLineEntry()
	correctedEntry.SetText(e.CorrectedText)
	correctedEntry.Disable()

	copyBtn := widget.NewButton("复制到剪贴板", func() {
		app.Clipboard().SetContent(text)
	})

	deleteBtn := widget.NewButton("删除此条", func() {
		dialog.ShowConfirm("确认删除", "确定删除这条记录？", func(ok bool) {
			if !ok {
				return
			}
			if err := DeleteHistory(e.ID); err != nil {
				dialog.ShowError(err, d)
				return
			}
			onDelete()
			d.Close()
		}, d)
	})

	content := container.NewVBox(
		infoLabel,
		widget.NewSeparator(),
		widget.NewLabel("原文："),
		rawEntry,
		widget.NewLabel("纠错后："),
		correctedEntry,
		container.NewHBox(copyBtn, deleteBtn),
	)

	d.SetContent(container.NewPadded(content))
	d.Show()
}
