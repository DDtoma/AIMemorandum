package domain

import (
    "testing"
    "time"
)

func Test_AC_NOT_002_CompletedItemRejectsSnooze(t *testing.T) {
    now := time.Now().UTC()
    item := Item{ID: NewID(), Kind: "task", TaskState: ptr("completed"), Revision: 3}
    _, err := Apply(item, Action{Action: "snooze", Payload: Payload{ReminderID: NewID(), RemindAt: &now, ScheduleRevision: NewID()}}, now)
    if Code(err) != "INVALID_STATE" { t.Fatalf("wanted INVALID_STATE, got %v", err) }
}

func Test_AC_API_001_EmptyCaptureRejected(t *testing.T) {
    item := Item{ID: NewID(), Kind: "capture", Timezone: "Asia/Shanghai", Source: Source{InputMethod: "text"}}
    if Code(Validate(item)) != "INVALID_CONTENT" { t.Fatal("empty record must be rejected") }
}

func Test_AC_NOT_001_ReminderDoesNotCompleteItem(t *testing.T) {
    now := time.Now().UTC()
    later := now.Add(time.Hour)
    item := Item{ID: NewID(), Kind: "capture", Title: "运行测试", BodyMD: "运行测试", Timezone: "Asia/Shanghai"}
    got, err := Apply(item, Action{Action: "capture_and_remind", Payload: Payload{ReminderID: NewID(), RemindAt: &later, ScheduleRevision: NewID()}}, now)
    if err != nil { t.Fatal(err) }
    if got.Kind != "task" || got.TaskState == nil || *got.TaskState != "todo" || len(got.Reminders) != 1 { t.Fatalf("bad transition: %+v", got) }
    done, err := Apply(got, Action{Action: "complete"}, now)
    if err != nil || done.Reminders[0].Enabled { t.Fatal("complete must cancel reminder") }
}
