// api 服务入口。开发模式：-data 指定数据目录，启动时打印注册码用于设备绑定。
package main

import (
	"crypto/rand"
	"encoding/hex"
	"flag"
	"log"
	"net/http"
	"time"

	"aimemorandum/services/api/internal/api"
	"aimemorandum/services/api/internal/store"
)

func randomCode() string {
	b := make([]byte, 4)
	if _, err := rand.Read(b); err != nil {
		log.Fatal(err)
	}
	return hex.EncodeToString(b)
}

func main() {
	addr := flag.String("addr", "127.0.0.1:8787", "监听地址")
	data := flag.String("data", "data", "数据目录（SQLite 与附件）")
	enroll := flag.String("enroll", "", "设备注册码；缺省自动生成并打印")
	flag.Parse()

	st, err := store.Open(*data)
	if err != nil {
		log.Fatalf("无法打开数据目录 %s: %v", *data, err)
	}
	defer st.Close()

	code := *enroll
	if code == "" {
		code = randomCode()
	}
	srv := &api.Server{Store: st, EnrollCode: code, MaxAttachment: 20971520}

	httpServer := &http.Server{
		Addr:              *addr,
		Handler:           srv.Routes(),
		ReadHeaderTimeout: 10 * time.Second,
	}
	log.Printf("AIMemorandum API 就绪 http://%s  注册码=%s  数据目录=%s", *addr, code, *data)
	log.Fatal(httpServer.ListenAndServe())
}
