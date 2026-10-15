package domain

import (
    "crypto/rand"
    "encoding/hex"
    "fmt"
    "regexp"
    "strings"
    "time"
    "unicode/utf8"
)

type Error struct { Status int; Name, Message string; Revision int64 }
func (e *Error) Error() string { return e.Name + ": " + e.Message }
func Fail(status int, name, message string) error { return &Error{Status: status, Name: name, Message: message} }
func Code(err error) string { if e, ok := err.(*Error); ok { return e.Name }; return "INTERNAL_ERROR" }
func ptr(s string) *string { return &s }
func NewID() string { b := make([]byte, 16); if _, err := rand.Read(b); err != nil { panic(err) }; b[6] = (b[6]&15)|64; b[8]=(b[8]&63)|128; h:=hex.EncodeToString(b); return h[:8]+"-"+h[8:12]+"-"+h[12:16]+"-"+h[16:20]+"-"+h[20:] }
var uuidRE = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`)
func IsID(s string) bool { return uuidRE.MatchString(s) }

type Source struct { InputMethod string `json:"input_method"`; ReportedApp string `json:"reported_app,omitempty"` }
type Attachment struct {
    ID string `json:"id"`; OwnerID string `json:"owner_id,omitempty"`; ItemID string `json:"item_id,omitempty"`
    MIME string `json:"mime"`; ByteSize int64 `json:"byte_size"`; SHA256 string `json:"sha256"`
    PixelWidth int `json:"pixel_width"`; PixelHeight int `json:"pixel_height"`; Position int `json:"position"`
    Caption string `json:"caption,omitempty"`; State string `json:"state"`; Revision int64 `json:"revision"`
}
type AttachmentInput struct {
    ID string `json:"id"`; MIME string `json:"mime"`; ByteSize int64 `json:"byte_size"`; SHA256 string `json:"sha256"`
    PixelWidth int `json:"pixel_width"`; PixelHeight int `json:"pixel_height"`; Position int `json:"position"`; Caption string `json:"caption,omitempty"`
}
type Reminder struct {
    ID string `json:"id"`; ItemID string `json:"item_id"`; RemindAt time.Time `json:"remind_at"`
    ScheduleRevision string `json:"schedule_revision"`; Purpose string `json:"purpose"`; Enabled bool `json:"enabled"`; Revision int64 `json:"revision"`
}
type Item struct {
    ID string `json:"id"`; OwnerID string `json:"owner_id"`; Kind string `json:"kind"`; TaskState *string `json:"task_state"`
    Title string `json:"title"`; TitleOrigin string `json:"title_origin"`; BodyMD string `json:"body_md"`; Timezone string `json:"timezone"`
    Revision int64 `json:"revision"`; CreatedAt time.Time `json:"created_at"`; UpdatedAt time.Time `json:"updated_at"`
    CompletedAt *time.Time `json:"completed_at"`; DeletedAt *time.Time `json:"deleted_at"`
    DueAt *time.Time `json:"due_at,omitempty"`; DueDate *string `json:"due_date,omitempty"`; PlannedFor *string `json:"planned_for,omitempty"`
    ReviewAt *time.Time `json:"review_at,omitempty"`; NextAction string `json:"next_action"`; ResumeNote string `json:"resume_note"`
    Source Source `json:"source"`; Attachments []Attachment `json:"attachments"`; Reminders []Reminder `json:"reminders"`
}
type Input struct {
    ID string `json:"id"`; Kind string `json:"kind"`; TaskState *string `json:"task_state,omitempty"`
    Title string `json:"title"`; TitleOrigin string `json:"title_origin"`; BodyMD string `json:"body_md"`; Timezone string `json:"timezone"`
    Source Source `json:"source"`; CapturedAt *time.Time `json:"captured_at,omitempty"`; Attachments []AttachmentInput `json:"attachments"`
}
func (in Input) Item(owner string, now time.Time) Item {
    item:=Item{ID:strings.ToLower(in.ID), OwnerID:owner, Kind:in.Kind, TaskState:in.TaskState, Title:in.Title, TitleOrigin:in.TitleOrigin, BodyMD:in.BodyMD, Timezone:in.Timezone, Source:in.Source, Revision:1, CreatedAt:now, UpdatedAt:now, Attachments:[]Attachment{}, Reminders:[]Reminder{}}
    if item.Kind=="task" && item.TaskState==nil { item.TaskState=ptr("todo") }
    if item.TitleOrigin=="" { item.TitleOrigin="user" }
    for _, a:=range in.Attachments { item.Attachments=append(item.Attachments,Attachment{ID:strings.ToLower(a.ID),OwnerID:owner,ItemID:item.ID,MIME:a.MIME,ByteSize:a.ByteSize,SHA256:a.SHA256,PixelWidth:a.PixelWidth,PixelHeight:a.PixelHeight,Position:a.Position,Caption:a.Caption,State:"pending",Revision:1}) }
    return item
}
func Validate(item Item) error {
    if !IsID(item.ID) { return Fail(422,"INVALID_CONTENT","事项 ID 无效") }
    if item.Kind!="capture" && item.Kind!="task" && item.Kind!="memo" { return Fail(422,"INVALID_CONTENT","类型无效") }
    if item.Kind=="task" { if item.TaskState==nil || !strings.Contains("|todo|in_progress|waiting|paused|completed|canceled|","|"+*item.TaskState+"|") { return Fail(422,"INVALID_STATE","任务状态无效") } } else if item.TaskState!=nil { return Fail(422,"INVALID_STATE","此类型没有任务状态") }
    if strings.TrimSpace(item.BodyMD)=="" && (item.TitleOrigin=="generated" || strings.TrimSpace(item.Title)=="") && len(item.Attachments)==0 { return Fail(422,"INVALID_CONTENT","请输入内容或提供图片") }
    if utf8.RuneCountInString(item.Title)>200 || len(item.BodyMD)>262144 { return Fail(422,"INVALID_CONTENT","内容超过限制") }
    if _,err:=time.LoadLocation(item.Timezone);err!=nil { return Fail(422,"INVALID_TIME","时区无效") }
    if item.DueAt!=nil && item.DueDate!=nil { return Fail(422,"INVALID_TIME","截止日期与时刻不能同时设置") }
    if len(item.Attachments)>10 { return Fail(413,"ATTACHMENT_TOO_LARGE","最多10张图片") }
    var total int64; ids:=map[string]bool{}
    for _,a:=range item.Attachments {
        if !IsID(a.ID)||ids[strings.ToLower(a.ID)]||a.Position<0||(a.MIME!="image/png"&&a.MIME!="image/jpeg")||a.ByteSize<=0||a.ByteSize>20971520||a.PixelWidth<=0||a.PixelHeight<=0||int64(a.PixelWidth)*int64(a.PixelHeight)>40000000 { return Fail(422,"INVALID_CONTENT","图片声明无效或超限") }
        if len(a.SHA256)!=64 { return Fail(422,"INVALID_CONTENT","图片哈希无效") }; if _,err:=hex.DecodeString(a.SHA256);err!=nil { return Fail(422,"INVALID_CONTENT","图片哈希无效") }
        ids[strings.ToLower(a.ID)]=true; total+=a.ByteSize
    }
    if total>52428800 { return Fail(413,"ATTACHMENT_TOO_LARGE","每条记录图片总量最多50 MiB") }
    return nil
}
type Payload struct {
    Kind string `json:"kind,omitempty"`; ReminderID string `json:"reminder_id,omitempty"`; RemindAt *time.Time `json:"remind_at,omitempty"`
    ScheduleRevision string `json:"schedule_revision,omitempty"`; ExpectedScheduleRevision string `json:"expected_schedule_revision,omitempty"`
}
type Action struct { OperationID string `json:"operation_id"`; BaseRevision int64 `json:"base_revision"`; Action string `json:"action"`; Payload Payload `json:"payload"`; ItemID string `json:"-"` }
var SupportedActions = []string{"complete","convert","capture_and_remind","set_reminder","snooze","remove_reminder","reopen","delete"}
func Apply(item Item, action Action, now time.Time) (Item,error) {
    if item.DeletedAt!=nil { return item,Fail(422,"INVALID_STATE","事项已删除") }
    ended:=item.TaskState!=nil&&(*item.TaskState=="completed"||*item.TaskState=="canceled")
    switch action.Action {
    case "complete":
        if item.Kind!="task"||ended { return item,Fail(422,"INVALID_STATE","只有未结束待办可以完成") }; item.TaskState=ptr("completed");item.CompletedAt=&now;disable(&item)
    case "reopen":
        if !ended { return item,Fail(422,"INVALID_STATE","事项尚未结束") };item.TaskState=ptr("todo");item.CompletedAt=nil
    case "delete": item.DeletedAt=&now;disable(&item)
    case "convert":
        if ended || (action.Payload.Kind!="task"&&action.Payload.Kind!="memo") { return item,Fail(422,"INVALID_STATE","转换类型无效") }
        item.Kind=action.Payload.Kind;if item.Kind=="task" { item.TaskState=ptr("todo") } else { item.TaskState=nil;disable(&item) }
    case "set_reminder","capture_and_remind","snooze":
        if ended || (item.Kind!="task"&&action.Action!="capture_and_remind") { return item,Fail(422,"INVALID_STATE","结束的事项不能设置提醒") }
        p:=action.Payload;if !IsID(p.ReminderID)||!IsID(p.ScheduleRevision)||p.RemindAt==nil||!p.RemindAt.After(now) { return item,Fail(422,"INVALID_TIME","请选择未来的有效提醒时间") }
        index:=-1;for i,r:=range item.Reminders { if r.ID==p.ReminderID { index=i;break } }
        if action.Action=="snooze"&&(index<0||!item.Reminders[index].Enabled||item.Reminders[index].ScheduleRevision!=p.ExpectedScheduleRevision) { return item,Fail(409,"STALE_SCHEDULE","提醒已发生变化") }
        count:=0;for _,r:=range item.Reminders { if r.Enabled { count++ } }; if index<0&&count>=8 { return item,Fail(422,"INVALID_CONTENT","每项最多8条提醒") }
        if action.Action=="capture_and_remind" { item.Kind="task";item.TaskState=ptr("todo") }
        rule:=Reminder{ID:p.ReminderID,ItemID:item.ID,RemindAt:p.RemindAt.UTC(),ScheduleRevision:p.ScheduleRevision,Purpose:"explicit",Enabled:true,Revision:1}
        if index>=0 { rule.Revision=item.Reminders[index].Revision+1;item.Reminders[index]=rule } else { item.Reminders=append(item.Reminders,rule) }
    case "remove_reminder":
        found:=false;for i,r:=range item.Reminders { if r.ID==action.Payload.ReminderID { if r.ScheduleRevision!=action.Payload.ExpectedScheduleRevision { return item,Fail(409,"STALE_SCHEDULE","提醒已发生变化") };item.Reminders[i].Enabled=false;item.Reminders[i].Revision++;found=true } };if !found { return item,Fail(404,"NOT_FOUND","未找到提醒") }
    default:return item,Fail(422,"UNSUPPORTED_ACTION",fmt.Sprintf("当前版本不支持 %s",action.Action))
    }
    item.Revision++;item.UpdatedAt=now;return item,nil
}
func disable(item *Item) { for i:=range item.Reminders { if item.Reminders[i].Enabled { item.Reminders[i].Enabled=false;item.Reminders[i].Revision++ } } }
