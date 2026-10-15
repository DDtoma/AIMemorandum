// Package api 实现 Phase 1 打通所需的 HTTP 接口：设备注册、事项创建、领域动作、
// 附件上传/下载与增量同步。错误统一为 {"error":{"name","message"}} 信封。
package api

import (
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"aimemorandum/services/api/internal/domain"
	"aimemorandum/services/api/internal/store"
)

type Server struct {
	Store         *store.Store
	EnrollCode    string
	MaxAttachment int64
}

func (s *Server) Routes() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/v1/capabilities", s.capabilities)
	mux.HandleFunc("POST /api/v1/device-enrollments", s.enroll)
	mux.HandleFunc("POST /api/v1/items", s.auth(s.createItem))
	mux.HandleFunc("POST /api/v1/items/{id}/actions", s.auth(s.action))
	mux.HandleFunc("GET /api/v1/items/{id}", s.auth(s.getItem))
	mux.HandleFunc("PUT /api/v1/items/{id}", s.auth(s.updateItem))
	mux.HandleFunc("PUT /api/v1/attachments/{id}/content", s.auth(s.putAttachment))
	mux.HandleFunc("GET /api/v1/attachments/{id}/content", s.auth(s.getAttachment))
	mux.HandleFunc("GET /api/v1/sync", s.auth(s.pull))
	return mux
}

// ---------- 基础设施 ----------

type errorBody struct {
	Error struct {
		Name    string `json:"name"`
		Message string `json:"message"`
	} `json:"error"`
	Item *domain.Item `json:"item,omitempty"`
}

func writeError(w http.ResponseWriter, status int, name, message string, item *domain.Item) {
	body := errorBody{Item: item}
	body.Error.Name = name
	body.Error.Message = message
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(body)
}

func writeJSON(w http.ResponseWriter, status int, payload any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(payload)
}

func (s *Server) auth(next func(w http.ResponseWriter, r *http.Request, owner string)) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		token := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
		if token == "" {
			writeError(w, 401, "UNAUTHORIZED", "缺少设备凭据", nil)
			return
		}
		owner, err := s.Store.DeviceOwner(token)
		if err != nil {
			writeError(w, 401, "UNAUTHORIZED", "设备凭据无效或已撤销", nil)
			return
		}
		next(w, r, owner)
	}
}

// ---------- 设备注册 ----------

func (s *Server) enroll(w http.ResponseWriter, r *http.Request) {
	var in struct {
		EnrollmentCode string `json:"enrollment_code"`
		DeviceName     string `json:"device_name"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 65536)).Decode(&in); err != nil {
		writeError(w, 400, "INVALID_CONTENT", "请求体不是有效 JSON", nil)
		return
	}
	if in.EnrollmentCode != s.EnrollCode {
		writeError(w, 403, "FORBIDDEN", "注册码无效", nil)
		return
	}
	name := strings.TrimSpace(in.DeviceName)
	if name == "" {
		name = "device-" + domain.NewID()[:8]
	}
	token := "dvc_" + domain.NewID() + domain.NewID()
	if err := s.Store.EnrollDevice(token, name); err != nil {
		writeError(w, 500, "INTERNAL_ERROR", "无法注册设备", nil)
		return
	}
	writeJSON(w, 201, map[string]any{"device_token": token, "device_name": name})
}

func (s *Server) capabilities(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, 200, map[string]any{
		"server_time": time.Now().UTC().Format(time.RFC3339Nano),
		"actions":     domain.SupportedActions,
		"attachments": map[string]any{
			"mimes":          []string{"image/png", "image/jpeg"},
			"max_per_file":   20971520,
			"max_per_item":   52428800,
			"max_count":      10,
		},
		"sync": map[string]any{"pull": true, "push_idempotent": true},
	})
}

// ---------- 事项 ----------

type createItemBody struct {
	OperationID string        `json:"operation_id"`
	Item        domain.Input  `json:"item"`
}

func (s *Server) createItem(w http.ResponseWriter, r *http.Request, owner string) {
	var in createItemBody
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&in); err != nil {
		writeError(w, 400, "INVALID_CONTENT", "请求体不是有效 JSON", nil)
		return
	}
	if !domain.IsID(in.OperationID) {
		writeError(w, 422, "INVALID_CONTENT", "operation_id 无效", nil)
		return
	}
	item := in.Item.Item(owner, time.Now().UTC())
	if err := domain.Validate(item); err != nil {
		if e, ok := err.(*domain.Error); ok {
			writeError(w, e.Status, e.Name, e.Message, nil)
			return
		}
		writeError(w, 422, "INVALID_CONTENT", err.Error(), nil)
		return
	}
	response, _, err := s.Store.CreateItem(in.OperationID, owner, item)
	if err != nil {
		writeStoreError(w, err, nil)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(201)
	_, _ = w.Write(response)
}

type actionBody struct {
	OperationID string       `json:"operation_id"`
	BaseRevision int64       `json:"base_revision"`
	Action       string      `json:"action"`
	Payload      domain.Payload `json:"payload"`
}

func (s *Server) action(w http.ResponseWriter, r *http.Request, owner string) {
	id := r.PathValue("id")
	var in actionBody
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&in); err != nil {
		writeError(w, 400, "INVALID_CONTENT", "请求体不是有效 JSON", nil)
		return
	}
	if !domain.IsID(in.OperationID) {
		writeError(w, 422, "INVALID_CONTENT", "operation_id 无效", nil)
		return
	}
	act := domain.Action{OperationID: in.OperationID, BaseRevision: in.BaseRevision, Action: in.Action, Payload: in.Payload, ItemID: id}
	response, applied, err := s.Store.ApplyAction(in.OperationID, owner, act)
	if err != nil {
		current, gErr := s.Store.GetItem(id)
		var currentPtr *domain.Item
		if gErr == nil {
			currentPtr = &current
		}
		writeStoreError(w, err, currentPtr)
		return
	}
	status := 200
	if applied {
		status = 200
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_, _ = w.Write(response)
}

func (s *Server) getItem(w http.ResponseWriter, r *http.Request, owner string) {
	item, err := s.Store.GetItem(r.PathValue("id"))
	if err != nil {
		writeStoreError(w, err, nil)
		return
	}
	writeJSON(w, 200, item)
}

type updateItemBody struct {
	OperationID  string `json:"operation_id"`
	BaseRevision int64  `json:"base_revision"`
	Title        string `json:"title"`
	BodyMd       string `json:"body_md"`
}

func (s *Server) updateItem(w http.ResponseWriter, r *http.Request, owner string) {
	id := r.PathValue("id")
	var in updateItemBody
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&in); err != nil {
		writeError(w, 400, "INVALID_CONTENT", "请求体不是有效 JSON", nil)
		return
	}
	if !domain.IsID(in.OperationID) {
		writeError(w, 422, "INVALID_CONTENT", "operation_id 无效", nil)
		return
	}
	if strings.TrimSpace(in.Title) == "" && strings.TrimSpace(in.BodyMd) == "" {
		writeError(w, 422, "INVALID_CONTENT", "标题与正文不能同时为空", nil)
		return
	}
	response, _, err := s.Store.UpdateItem(in.OperationID, owner, id, in.BaseRevision, in.Title, in.BodyMd)
	if err != nil {
		current, gErr := s.Store.GetItem(id)
		var currentPtr *domain.Item
		if gErr == nil {
			currentPtr = &current
		}
		writeStoreError(w, err, currentPtr)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(200)
	_, _ = w.Write(response)
}

func writeStoreError(w http.ResponseWriter, err error, item *domain.Item) {
	var dErr *domain.Error
	switch {
	case errors.Is(err, store.ErrNotFound):
		writeError(w, 404, "NOT_FOUND", "资源不存在", item)
	case errors.As(err, &dErr):
		writeError(w, dErr.Status, dErr.Name, dErr.Message, item)
	default:
		writeError(w, 500, "INTERNAL_ERROR", "服务端内部错误", nil)
	}
}

// ---------- 附件 ----------

func (s *Server) putAttachment(w http.ResponseWriter, r *http.Request, owner string) {
	id := r.PathValue("id")
	data, err := io.ReadAll(io.LimitReader(r.Body, s.MaxAttachment+1))
	if err != nil {
		writeError(w, 400, "INVALID_CONTENT", "无法读取附件内容", nil)
		return
	}
	if int64(len(data)) > s.MaxAttachment {
		writeError(w, 413, "ATTACHMENT_TOO_LARGE", "附件超过服务端上限", nil)
		return
	}
	att, err := s.Store.PutAttachmentContent(id, data)
	if err != nil {
		writeStoreError(w, err, nil)
		return
	}
	writeJSON(w, 200, map[string]any{"attachment": att})
}

func (s *Server) getAttachment(w http.ResponseWriter, r *http.Request, owner string) {
	id := r.PathValue("id")
	row, err := s.Store.Attachment(id)
	if err != nil {
		writeStoreError(w, err, nil)
		return
	}
	data, err := readFileLimited(s.Store.AttachmentPath(id), row.ByteSize)
	if err != nil {
		writeError(w, 404, "NOT_FOUND", "附件内容缺失", nil)
		return
	}
	w.Header().Set("Content-Type", row.MIME)
	w.Header().Set("Content-Length", strconv.Itoa(int(row.ByteSize)))
	_, _ = w.Write(data)
}

func readFileLimited(path string, size int64) ([]byte, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	data := make([]byte, size)
	n, err := io.ReadFull(f, data)
	if err != nil && err != io.ErrUnexpectedEOF {
		return nil, err
	}
	return data[:n], nil
}

// ---------- 同步 ----------

func (s *Server) pull(w http.ResponseWriter, r *http.Request, owner string) {
	cursor := int64(0)
	if raw := r.URL.Query().Get("cursor"); raw != "" {
		var parsed int64
		for _, ch := range raw {
			if ch < '0' || ch > '9' {
				writeError(w, 422, "INVALID_CURSOR", "同步游标无效", nil)
				return
			}
			parsed = parsed*10 + int64(ch-'0')
		}
		cursor = parsed
	}
	limit := 200
	events, err := s.Store.EventsSince(cursor, limit)
	if err != nil {
		writeStoreError(w, err, nil)
		return
	}
	ids := make([]string, 0, len(events))
	seen := map[string]bool{}
	newCursor := cursor
	for _, e := range events {
		newCursor = e.Seq
		if !seen[e.ItemID] {
			seen[e.ItemID] = true
			ids = append(ids, e.ItemID)
		}
	}
	items, err := s.Store.Items(ids)
	if err != nil {
		writeStoreError(w, err, nil)
		return
	}
	writeJSON(w, 200, map[string]any{"cursor": strconv.FormatInt(newCursor, 10), "items": items})
}
