// 集成测试：以 httptest 起真实路由，覆盖 创建→附件上传→动作→同步拉取 的完整闭环。
package api

import (
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"aimemorandum/services/api/internal/store"
)

func newTestServer(t *testing.T) (*httptest.Server, string) {
	t.Helper()
	st, err := store.Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { st.Close() })
	srv := &Server{Store: st, EnrollCode: "test-code", MaxAttachment: 20971520}
	ts := httptest.NewServer(srv.Routes())
	t.Cleanup(ts.Close)

	// 注册一台设备拿 token
	body, _ := json.Marshal(map[string]string{"enrollment_code": "test-code", "device_name": "ci-mac"})
	resp, err := http.Post(ts.URL+"/api/v1/device-enrollments", "application/json", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 201 {
		b, _ := io.ReadAll(resp.Body)
		t.Fatalf("enroll status=%d body=%s", resp.StatusCode, b)
	}
	var out struct {
		DeviceToken string `json:"device_token"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		t.Fatal(err)
	}
	return ts, out.DeviceToken
}

func call(t *testing.T, token, method, path string, body []byte, headers map[string]string) (int, []byte) {
	t.Helper()
	req, err := http.NewRequest(method, path, bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	req.Header.Set("Content-Type", "application/json")
	for k, v := range headers {
		req.Header.Set(k, v)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	data, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, data
}

func id() string {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		panic(err)
	}
	b[6] = (b[6] & 15) | 64
	b[8] = (b[8] & 63) | 128
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

func Test_FullLoop_CreateUploadActionSync(t *testing.T) {
	ts, token := newTestServer(t)

	itemID := strings.ToLower(id())
	attID := strings.ToLower(id())
	png := bytes.Repeat([]byte{0x89, 'P', 'N', 'G'}, 32) // 内容任意，仅校验哈希与大小
	sum := sha256.Sum256(png)

	createBody, _ := json.Marshal(map[string]any{
		"operation_id": id(),
		"item": map[string]any{
			"id": itemID, "kind": "capture", "title": "截图核对账单", "title_origin": "generated",
			"body_md": "", "timezone": "Asia/Shanghai",
			"source": map[string]string{"input_method": "screenshot"},
			"attachments": []map[string]any{{
				"id": attID, "mime": "image/png", "byte_size": len(png),
				"sha256": hex.EncodeToString(sum[:]), "pixel_width": 800, "pixel_height": 600, "position": 0,
			}},
		},
	})
	status, resp := call(t, token, "POST", ts.URL+"/api/v1/items", createBody, nil)
	if status != 201 {
		t.Fatalf("create status=%d body=%s", status, resp)
	}

	// 幂等：同 operation_id 重放返回 200 与相同响应
	status, resp2 := call(t, token, "POST", ts.URL+"/api/v1/items", createBody, nil)
	if status != 201 || !bytes.Equal(resp, resp2) {
		t.Fatalf("idempotent replay mismatch status=%d", status)
	}

	// 未上传前下载应 404
	if status, _ = call(t, token, "GET", ts.URL+"/api/v1/attachments/"+attID+"/content", nil, nil); status != 404 {
		t.Fatalf("expected 404 before upload, got %d", status)
	}

	// 哈希不一致应被拒绝
	bad := append([]byte(nil), png...)
	bad[0] ^= 0xff
	status, _ = call(t, token, "PUT", ts.URL+"/api/v1/attachments/"+attID+"/content", bad, map[string]string{"Content-Type": "image/png"})
	if status != 422 {
		t.Fatalf("expected 422 for bad hash, got %d", status)
	}

	// 正确上传
	status, resp = call(t, token, "PUT", ts.URL+"/api/v1/attachments/"+attID+"/content", png, map[string]string{"Content-Type": "image/png"})
	if status != 200 {
		t.Fatalf("upload status=%d body=%s", status, resp)
	}
	status, got := call(t, token, "GET", ts.URL+"/api/v1/attachments/"+attID+"/content", nil, nil)
	if status != 200 || !bytes.Equal(got, png) {
		t.Fatalf("download mismatch status=%d len=%d", status, len(got))
	}

	// 动作：设置提醒（capture_and_remind）
	later := time.Now().UTC().Add(time.Hour).Format(time.RFC3339Nano)
	actionBody, _ := json.Marshal(map[string]any{
		"operation_id": id(), "base_revision": 1, "action": "capture_and_remind",
		"payload": map[string]any{"reminder_id": strings.ToLower(id()), "remind_at": later, "schedule_revision": strings.ToLower(id())},
	})
	status, resp = call(t, token, "POST", ts.URL+"/api/v1/items/"+itemID+"/actions", actionBody, nil)
	if status != 200 {
		t.Fatalf("action status=%d body=%s", status, resp)
	}
	var actionOut struct {
		Item struct {
			Revision  int64  `json:"revision"`
			Kind      string `json:"kind"`
			TaskState string `json:"task_state"`
		} `json:"item"`
	}
	if err := json.Unmarshal(resp, &actionOut); err != nil {
		t.Fatal(err)
	}
	if actionOut.Item.Kind != "task" || actionOut.Item.TaskState != "todo" || actionOut.Item.Revision != 2 {
		t.Fatalf("unexpected action result %+v", actionOut.Item)
	}

	// 版本冲突：旧 base_revision 应 409 并回传当前版本
	stale, _ := json.Marshal(map[string]any{
		"operation_id": id(), "base_revision": 1, "action": "complete", "payload": map[string]any{},
	})
	status, resp = call(t, token, "POST", ts.URL+"/api/v1/items/"+itemID+"/actions", stale, nil)
	if status != 409 {
		t.Fatalf("expected 409 conflict, got %d body=%s", status, resp)
	}
	var conflict struct {
		Error struct {
			Name string `json:"name"`
		} `json:"error"`
		Item struct {
			Revision int64 `json:"revision"`
		} `json:"item"`
	}
	if err := json.Unmarshal(resp, &conflict); err != nil || conflict.Error.Name != "REVISION_CONFLICT" || conflict.Item.Revision != 2 {
		t.Fatalf("bad conflict body %s", resp)
	}

	// 同步拉取：cursor=0 应包含该事项，游标前移
	status, resp = call(t, token, "GET", ts.URL+"/api/v1/sync?cursor=0", nil, nil)
	if status != 200 {
		t.Fatalf("sync status=%d", status)
	}
	var page struct {
		Cursor string `json:"cursor"`
		Items  []struct {
			ID          string `json:"id"`
			Attachments []struct {
				State string `json:"state"`
			} `json:"attachments"`
		} `json:"items"`
	}
	if err := json.Unmarshal(resp, &page); err != nil {
		t.Fatal(err)
	}
	if page.Cursor == "0" || len(page.Items) == 0 {
		t.Fatalf("bad sync page cursor=%s items=%d", page.Cursor, len(page.Items))
	}
	found := false
	for _, it := range page.Items {
		if it.ID == itemID {
			found = true
			if len(it.Attachments) != 1 || it.Attachments[0].State != "ready" {
				t.Fatalf("attachment state not ready in sync page")
			}
		}
	}
	if !found {
		t.Fatal("item missing from sync page")
	}

	// 无凭据应 401
	status, _ = call(t, "", "GET", ts.URL+"/api/v1/sync?cursor=0", nil, nil)
	if status != 401 {
		t.Fatalf("expected 401 without token, got %d", status)
	}
}

func Test_UpdateItem_Put(t *testing.T) {
	ts, token := newTestServer(t)

	itemID := strings.ToLower(id())
	createBody, _ := json.Marshal(map[string]any{
		"operation_id": id(),
		"item": map[string]any{
			"id": itemID, "kind": "task", "title": "原始标题", "title_origin": "user",
			"body_md": "原始正文", "timezone": "Asia/Shanghai",
			"source": map[string]string{"input_method": "text"},
		},
	})
	status, resp := call(t, token, "POST", ts.URL+"/api/v1/items", createBody, nil)
	if status != 201 {
		t.Fatalf("create status=%d body=%s", status, resp)
	}

	// 正常更新：revision 1 → 2，标题与正文生效
	updateBody, _ := json.Marshal(map[string]any{
		"operation_id": id(), "base_revision": 1, "title": "修改后的标题", "body_md": "修改后的正文",
	})
	status, resp = call(t, token, "PUT", ts.URL+"/api/v1/items/"+itemID, updateBody, nil)
	if status != 200 {
		t.Fatalf("update status=%d body=%s", status, resp)
	}
	var out struct {
		Item struct {
			Revision    int64  `json:"revision"`
			Title       string `json:"title"`
			BodyMd      string `json:"body_md"`
			TitleOrigin string `json:"title_origin"`
		} `json:"item"`
	}
	if err := json.Unmarshal(resp, &out); err != nil {
		t.Fatal(err)
	}
	if out.Item.Revision != 2 || out.Item.Title != "修改后的标题" || out.Item.BodyMd != "修改后的正文" || out.Item.TitleOrigin != "user" {
		t.Fatalf("unexpected update result %+v", out.Item)
	}

	// 幂等重放：同 operation_id 返回相同响应
	status, replay := call(t, token, "PUT", ts.URL+"/api/v1/items/"+itemID, updateBody, nil)
	if status != 200 || !bytes.Equal(resp, replay) {
		t.Fatalf("update replay mismatch status=%d", status)
	}

	// 过期 base_revision 应 409
	stale, _ := json.Marshal(map[string]any{
		"operation_id": id(), "base_revision": 1, "title": "X", "body_md": "Y",
	})
	status, resp = call(t, token, "PUT", ts.URL+"/api/v1/items/"+itemID, stale, nil)
	if status != 409 {
		t.Fatalf("expected 409 conflict, got %d body=%s", status, resp)
	}

	// 全空应 422
	emptyAll, _ := json.Marshal(map[string]any{
		"operation_id": id(), "base_revision": 2, "title": " ", "body_md": "",
	})
	status, _ = call(t, token, "PUT", ts.URL+"/api/v1/items/"+itemID, emptyAll, nil)
	if status != 422 {
		t.Fatalf("expected 422 for empty update, got %d", status)
	}

	// 更新应进入同步事件流（cursor=0 能拉到 revision 2 的版本）
	status, resp = call(t, token, "GET", ts.URL+"/api/v1/sync?cursor=0", nil, nil)
	if status != 200 {
		t.Fatalf("sync status=%d", status)
	}
	var page struct {
		Items []struct {
			ID       string `json:"id"`
			Revision int64  `json:"revision"`
			Title    string `json:"title"`
		} `json:"items"`
	}
	if err := json.Unmarshal(resp, &page); err != nil {
		t.Fatal(err)
	}
	found := false
	for _, it := range page.Items {
		if it.ID == itemID && it.Revision == 2 && it.Title == "修改后的标题" {
			found = true
		}
	}
	if !found {
		t.Fatalf("updated item missing from sync page %s", resp)
	}
}

func Test_AC_API_001_CreateValidations(t *testing.T) {
	ts, token := newTestServer(t)

	// 空内容拒绝
	empty, _ := json.Marshal(map[string]any{
		"operation_id": id(),
		"item": map[string]any{
			"id": strings.ToLower(id()), "kind": "capture", "title": "", "title_origin": "generated",
			"body_md": "", "timezone": "Asia/Shanghai", "source": map[string]string{"input_method": "text"},
		},
	})
	status, resp := call(t, token, "POST", ts.URL+"/api/v1/items", empty, nil)
	if status != 422 || !bytes.Contains(resp, []byte("INVALID_CONTENT")) {
		t.Fatalf("expected 422 INVALID_CONTENT, got %d %s", status, resp)
	}

	// 非法注册码拒绝
	bad, _ := json.Marshal(map[string]string{"enrollment_code": "wrong", "device_name": "x"})
	req, _ := http.NewRequest("POST", ts.URL+"/api/v1/device-enrollments", bytes.NewReader(bad))
	respHTTP, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	respHTTP.Body.Close()
	if respHTTP.StatusCode != 403 {
		t.Fatalf("expected 403 for bad enrollment, got %d", respHTTP.StatusCode)
	}
}
