package main

import (
	"database/sql"
	"log/slog"
	"os"
	"path/filepath"
	"time"

	_ "github.com/mattn/go-sqlite3"
)

type HistoryEntry struct {
	ID            int64
	CreatedAt     time.Time
	RawText       string
	CorrectedText string
	ASRProvider   string
}

var db *sql.DB

// InitHistory 初始化历史记录 SQLite 数据库，首次调用时创建表结构。
func InitHistory() error {
	dir := configDir()
	os.MkdirAll(dir, 0o755) //nolint:errcheck,gosec // best-effort directory creation

	var err error
	db, err = sql.Open("sqlite3", filepath.Join(dir, "history.db"))
	if err != nil {
		return err
	}

	_, err = db.Exec(`CREATE TABLE IF NOT EXISTS history (
		id           INTEGER PRIMARY KEY AUTOINCREMENT,
		created_at   INTEGER NOT NULL,
		raw_text     TEXT NOT NULL,
		corrected_text TEXT NOT NULL,
		asr_provider TEXT NOT NULL
	)`)
	return err
}

// SaveHistory 保存一条识别记录，db 未初始化时静默返回。
func SaveHistory(raw, corrected, provider string) {
	if db == nil {
		return
	}
	_, err := db.Exec(
		`INSERT INTO history (created_at, raw_text, corrected_text, asr_provider) VALUES (?, ?, ?, ?)`,
		time.Now().Unix(), raw, corrected, provider,
	)
	if err != nil {
		slog.Warn("保存历史记录失败", "error", err)
	}
}

// LoadHistory 按时间倒序返回最近 limit 条历史记录。
func LoadHistory(limit int) ([]HistoryEntry, error) {
	if db == nil {
		return []HistoryEntry{}, nil
	}
	rows, err := db.Query(
		`SELECT id, created_at, raw_text, corrected_text, asr_provider FROM history ORDER BY created_at DESC LIMIT ?`,
		limit,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close() //nolint:errcheck // deferred rows close

	entries := make([]HistoryEntry, 0, limit)
	for rows.Next() {
		var e HistoryEntry
		var ts int64
		if err := rows.Scan(&e.ID, &ts, &e.RawText, &e.CorrectedText, &e.ASRProvider); err != nil {
			continue
		}
		e.CreatedAt = time.Unix(ts, 0)
		entries = append(entries, e)
	}
	if err := rows.Err(); err != nil {
		return entries, err
	}
	return entries, nil
}

// DeleteHistory 删除指定 ID 的历史记录。
func DeleteHistory(id int64) error {
	if db == nil {
		return nil
	}
	_, err := db.Exec(`DELETE FROM history WHERE id = ?`, id)
	return err
}

// ClearHistory 清空全部历史记录。
func ClearHistory() error {
	if db == nil {
		return nil
	}
	_, err := db.Exec(`DELETE FROM history`)
	return err
}
