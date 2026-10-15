// Package store 提供服务端持久化：设备、事项、事件（同步游标）、操作幂等与附件。
// Phase 1 打通阶段使用本地 SQLite（modernc.org/sqlite，纯 Go）；生产目标为 PostgreSQL，
// 存储接口保持薄封装，后续可替换驱动而不改动 API 层。
package store

import (
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"aimemorandum/services/api/internal/domain"

	_ "modernc.org/sqlite"
)

var ErrNotFound = errors.New("NOT_FOUND")
var ErrExists = errors.New("ALREADY_EXISTS")

type Store struct {
	mu  sync.Mutex
	db  *sql.DB
	dir string
}

func Open(dir string) (*Store, error) {
	if err := os.MkdirAll(filepath.Join(dir, "attachments"), 0o755); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", filepath.Join(dir, "api.sqlite"))
	if err != nil {
		return nil, err
	}
	// 单写者模型：现代 SQLite 驱动按连接串行化写，限制为 1 连接避免 SQLITE_BUSY。
	db.SetMaxOpenConns(1)
	s := &Store{db: db, dir: dir}
	if err := s.migrate(); err != nil {
		db.Close()
		return nil, err
	}
	return s, nil
}

func (s *Store) Close() error { return s.db.Close() }

func (s *Store) migrate() error {
	stmts := []string{
		`PRAGMA journal_mode=WAL`,
		`PRAGMA synchronous=FULL`,
		`CREATE TABLE IF NOT EXISTS devices (
			token TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS items (
			id TEXT PRIMARY KEY, owner TEXT NOT NULL, json TEXT NOT NULL,
			revision INTEGER NOT NULL, updated_at TEXT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS events (
			seq INTEGER PRIMARY KEY AUTOINCREMENT, item_id TEXT NOT NULL,
			kind TEXT NOT NULL, committed_at TEXT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS ops (
			id TEXT PRIMARY KEY, owner TEXT NOT NULL, item_id TEXT NOT NULL,
			result_revision INTEGER NOT NULL, response TEXT NOT NULL, created_at TEXT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS attachments (
			id TEXT PRIMARY KEY, owner TEXT NOT NULL, item_id TEXT NOT NULL,
			mime TEXT NOT NULL, byte_size INTEGER NOT NULL, sha256 TEXT NOT NULL,
			state TEXT NOT NULL, created_at TEXT NOT NULL)`,
		`CREATE INDEX IF NOT EXISTS attachments_item ON attachments(item_id)`,
	}
	for _, stmt := range stmts {
		if _, err := s.db.Exec(stmt); err != nil {
			return fmt.Errorf("migrate: %w", err)
		}
	}
	return nil
}

func (s *Store) EnrollDevice(token, name string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	_, err := s.db.Exec(`INSERT INTO devices(token,name,created_at) VALUES (?,?,?)`,
		token, name, time.Now().UTC().Format(time.RFC3339Nano))
	return err
}

func (s *Store) DeviceOwner(token string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var name string
	err := s.db.QueryRow(`SELECT name FROM devices WHERE token=?`, token).Scan(&name)
	if errors.Is(err, sql.ErrNoRows) {
		return "", ErrNotFound
	}
	return name, err
}

func (s *Store) GetItem(id string) (domain.Item, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.getItem(id)
}

func (s *Store) getItem(id string) (domain.Item, error) {
	var raw string
	err := s.db.QueryRow(`SELECT json FROM items WHERE id=?`, id).Scan(&raw)
	if errors.Is(err, sql.ErrNoRows) {
		return domain.Item{}, ErrNotFound
	}
	if err != nil {
		return domain.Item{}, err
	}
	var item domain.Item
	if err := json.Unmarshal([]byte(raw), &item); err != nil {
		return domain.Item{}, err
	}
	return item, nil
}

// CreateItem 幂等创建：同 operation_id 重复提交返回首次响应。
func (s *Store) CreateItem(opID, owner string, item domain.Item) (response []byte, created bool, err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if resp, ok, err := s.getOp(opID, owner); err == nil && ok {
		return resp, false, nil
	}
	if _, err := s.getItem(item.ID); err == nil {
		return nil, false, domain.Fail(409, "ITEM_EXISTS", "事项已存在")
	}
	raw, err := json.Marshal(item)
	if err != nil {
		return nil, false, err
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	tx, err := s.db.Begin()
	if err != nil {
		return nil, false, err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`INSERT INTO items(id,owner,json,revision,updated_at) VALUES (?,?,?,?,?)`,
		item.ID, owner, string(raw), item.Revision, now); err != nil {
		return nil, false, err
	}
	if _, err := tx.Exec(`INSERT INTO events(item_id,kind,committed_at) VALUES (?,?,?)`,
		item.ID, "created", now); err != nil {
		return nil, false, err
	}
	for _, a := range item.Attachments {
		if _, err := tx.Exec(`INSERT INTO attachments(id,owner,item_id,mime,byte_size,sha256,state,created_at) VALUES (?,?,?,?,?,?,?,?)`,
			a.ID, owner, item.ID, a.MIME, a.ByteSize, a.SHA256, "pending", now); err != nil {
			return nil, false, err
		}
	}
	response, err = json.Marshal(map[string]any{"operation_id": opID, "item": item})
	if err != nil {
		return nil, false, err
	}
	if _, err := tx.Exec(`INSERT INTO ops(id,owner,item_id,result_revision,response,created_at) VALUES (?,?,?,?,?,?)`,
		opID, owner, item.ID, item.Revision, string(response), now); err != nil {
		return nil, false, err
	}
	if err := tx.Commit(); err != nil {
		return nil, false, err
	}
	return response, true, nil
}

// ApplyAction 执行带版本校验的领域动作，幂等返回。
func (s *Store) ApplyAction(opID, owner string, action domain.Action) (response []byte, applied bool, err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if resp, ok, err := s.getOp(opID, owner); err == nil && ok {
		return resp, false, nil
	}
	item, err := s.getItem(action.ItemID)
	if err != nil {
		return nil, false, err
	}
	if action.BaseRevision != item.Revision {
		body, mErr := json.Marshal(conflictBody(item))
		if mErr != nil {
			return nil, false, mErr
		}
		return body, false, domain.Fail(409, "REVISION_CONFLICT", "事项已在其他设备更新")
	}
	now := time.Now().UTC()
	updated, err := domain.Apply(item, action, now)
	if err != nil {
		return nil, false, err
	}
	raw, err := json.Marshal(updated)
	if err != nil {
		return nil, false, err
	}
	stamp := now.Format(time.RFC3339Nano)
	tx, err := s.db.Begin()
	if err != nil {
		return nil, false, err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`UPDATE items SET json=?,revision=?,updated_at=? WHERE id=?`,
		string(raw), updated.Revision, stamp, updated.ID); err != nil {
		return nil, false, err
	}
	if _, err := tx.Exec(`INSERT INTO events(item_id,kind,committed_at) VALUES (?,?,?)`,
		updated.ID, "action:"+action.Action, stamp); err != nil {
		return nil, false, err
	}
	// 动作可能禁用提醒：同步附件表状态之外，还使提醒出现规则与事项一致（提醒表由查询时读取事项 JSON，无需单独维护）。
	response, err = json.Marshal(map[string]any{"operation_id": opID, "item": updated})
	if err != nil {
		return nil, false, err
	}
	if _, err := tx.Exec(`INSERT INTO ops(id,owner,item_id,result_revision,response,created_at) VALUES (?,?,?,?,?,?)`,
		opID, owner, updated.ID, updated.Revision, string(response), stamp); err != nil {
		return nil, false, err
	}
	if err := tx.Commit(); err != nil {
		return nil, false, err
	}
	return response, true, nil
}

// UpdateItem 更新事项标题与正文：base_revision 校验防丢更新，幂等返回。
func (s *Store) UpdateItem(opID, owner, id string, baseRevision int64, title, bodyMd string) (response []byte, applied bool, err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if resp, ok, err := s.getOp(opID, owner); err == nil && ok {
		return resp, false, nil
	}
	item, err := s.getItem(id)
	if err != nil {
		return nil, false, err
	}
	if baseRevision != item.Revision {
		body, mErr := json.Marshal(conflictBody(item))
		if mErr != nil {
			return nil, false, mErr
		}
		return body, false, domain.Fail(409, "REVISION_CONFLICT", "事项已在其他设备更新")
	}
	updated := item
	updated.Title = title
	updated.BodyMD = bodyMd
	updated.TitleOrigin = "user"
	updated.Revision = item.Revision + 1
	now := time.Now().UTC()
	updated.UpdatedAt = now
	stamp := now.Format(time.RFC3339Nano)
	raw, err := json.Marshal(updated)
	if err != nil {
		return nil, false, err
	}
	tx, err := s.db.Begin()
	if err != nil {
		return nil, false, err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`UPDATE items SET json=?,revision=?,updated_at=? WHERE id=?`,
		string(raw), updated.Revision, stamp, updated.ID); err != nil {
		return nil, false, err
	}
	if _, err := tx.Exec(`INSERT INTO events(item_id,kind,committed_at) VALUES (?,?,?)`,
		updated.ID, "updated", stamp); err != nil {
		return nil, false, err
	}
	response, err = json.Marshal(map[string]any{"operation_id": opID, "item": updated})
	if err != nil {
		return nil, false, err
	}
	if _, err := tx.Exec(`INSERT INTO ops(id,owner,item_id,result_revision,response,created_at) VALUES (?,?,?,?,?,?)`,
		opID, owner, updated.ID, updated.Revision, string(response), stamp); err != nil {
		return nil, false, err
	}
	if err := tx.Commit(); err != nil {
		return nil, false, err
	}
	return response, true, nil
}

func conflictBody(item domain.Item) map[string]any {
	return map[string]any{
		"error": map[string]any{"name": "REVISION_CONFLICT", "message": "事项已在其他设备更新"},
		"item":  item,
	}
}

func (s *Store) getOp(opID, owner string) ([]byte, bool, error) {
	var resp string
	var resultRevision int64
	err := s.db.QueryRow(`SELECT response,result_revision FROM ops WHERE id=? AND owner=?`, opID, owner).Scan(&resp, &resultRevision)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	return []byte(resp), true, nil
}

type AttachmentRow struct {
	ID       string
	ItemID   string
	MIME     string
	ByteSize int64
	SHA256   string
	State    string
}

func (s *Store) Attachment(id string) (AttachmentRow, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var row AttachmentRow
	err := s.db.QueryRow(`SELECT id,item_id,mime,byte_size,sha256,state FROM attachments WHERE id=?`, id).
		Scan(&row.ID, &row.ItemID, &row.MIME, &row.ByteSize, &row.SHA256, &row.State)
	if errors.Is(err, sql.ErrNoRows) {
		return row, ErrNotFound
	}
	return row, err
}

// PutAttachmentContent 校验哈希与大小后落盘，并将事项 JSON 中附件置为 ready。
func (s *Store) PutAttachmentContent(id string, data []byte) (domain.Attachment, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var row AttachmentRow
	err := s.db.QueryRow(`SELECT id,item_id,mime,byte_size,sha256,state FROM attachments WHERE id=?`, id).
		Scan(&row.ID, &row.ItemID, &row.MIME, &row.ByteSize, &row.SHA256, &row.State)
	if errors.Is(err, sql.ErrNoRows) {
		return domain.Attachment{}, domain.Fail(404, "NOT_FOUND", "未找到附件声明")
	}
	if err != nil {
		return domain.Attachment{}, err
	}
	if row.State == "ready" {
		return domain.Attachment{}, domain.Fail(409, "INVALID_STATE", "附件已上传")
	}
	if int64(len(data)) != row.ByteSize {
		return domain.Attachment{}, domain.Fail(422, "INVALID_CONTENT", "附件大小与声明不一致")
	}
	sum := sha256.Sum256(data)
	if hex.EncodeToString(sum[:]) != row.SHA256 {
		return domain.Attachment{}, domain.Fail(422, "INVALID_CONTENT", "附件哈希与声明不一致")
	}
	// 先落盘并 fsync，再更新状态：崩溃后最多重复上传，不会出现状态 ready 而文件缺失。
	path := s.attachmentPath(id)
	if err := writeFileDurable(path, data); err != nil {
		return domain.Attachment{}, err
	}
	item, err := s.getItem(row.ItemID)
	if err != nil {
		return domain.Attachment{}, err
	}
	updated := item
	updated.Attachments = append([]domain.Attachment(nil), item.Attachments...)
	for i := range updated.Attachments {
		if updated.Attachments[i].ID == id {
			updated.Attachments[i].State = "ready"
		}
	}
	raw, err := json.Marshal(updated)
	if err != nil {
		return domain.Attachment{}, err
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	tx, err := s.db.Begin()
	if err != nil {
		return domain.Attachment{}, err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`UPDATE attachments SET state='ready' WHERE id=?`, id); err != nil {
		return domain.Attachment{}, err
	}
	if _, err := tx.Exec(`UPDATE items SET json=?,updated_at=? WHERE id=?`, string(raw), now, item.ID); err != nil {
		return domain.Attachment{}, err
	}
	if _, err := tx.Exec(`INSERT INTO events(item_id,kind,committed_at) VALUES (?,?,?)`, item.ID, "attachment:ready", now); err != nil {
		return domain.Attachment{}, err
	}
	if err := tx.Commit(); err != nil {
		return domain.Attachment{}, err
	}
	for i := range updated.Attachments {
		if updated.Attachments[i].ID == id {
			return updated.Attachments[i], nil
		}
	}
	return domain.Attachment{}, ErrNotFound
}

func (s *Store) AttachmentPath(id string) string { return s.attachmentPath(id) }
func (s *Store) attachmentPath(id string) string {
	return filepath.Join(s.dir, "attachments", id+".bin")
}

func writeFileDurable(path string, data []byte) error {
	tmp := path + ".tmp"
	f, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o600)
	if err != nil {
		return err
	}
	if _, err := f.Write(data); err != nil {
		f.Close()
		return err
	}
	if err := f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

type EventRow struct {
	Seq    int64
	ItemID string
}

// EventsSince 返回游标之后的事件页（按提交序），用于增量同步。
func (s *Store) EventsSince(seq int64, limit int) ([]EventRow, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	rows, err := s.db.Query(`SELECT seq,item_id FROM events WHERE seq>? ORDER BY seq LIMIT ?`, seq, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []EventRow
	for rows.Next() {
		var r EventRow
		if err := rows.Scan(&r.Seq, &r.ItemID); err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

func (s *Store) Items(ids []string) ([]domain.Item, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]domain.Item, 0, len(ids))
	for _, id := range ids {
		item, err := s.getItem(id)
		if err != nil {
			continue
		}
		out = append(out, item)
	}
	return out, nil
}
