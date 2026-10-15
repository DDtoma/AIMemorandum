import Foundation

struct TestFailure: Error { let message: String }
func XCTAssertEqual<T: Equatable>(_ actual: T, _ expected: T, file: StaticString = #filePath, line: UInt = #line) throws {
    if actual != expected { throw TestFailure(message: "\(file):\(line) expected \(expected), got \(actual)") }
}
func XCTAssertTrue(_ value: Bool) throws { try XCTAssertEqual(value, true) }
func XCTAssertFalse(_ value: Bool) throws { try XCTAssertEqual(value, false) }
func XCTUnwrap<T>(_ value: T?) throws -> T { guard let value else { throw TestFailure(message: "unexpected nil") }; return value }

let suite = CoreTests()
let cases: [(String, () throws -> Void)] = [
    ("AC-CAP-001 offline durability", suite.test_AC_CAP_001_OfflineCaptureSurvivesReopen),
    ("AC-NOT-001 complete cancels reminders", suite.test_AC_NOT_001_CompleteCancelsLocalReminderWithoutChangingDeadline),
    ("AC-SYNC-002 preserve local projection", suite.test_AC_SYNC_002_RemoteVersionDoesNotOverwritePendingLocalText),
    ("AC-CAP-003 input composition", suite.test_AC_CAP_003_CompositionDoesNotSubmit),
    ("AC-SYNC-001 push create acknowledged", suite.test_AC_SYNC_001_PushCreateAcknowledged),
    ("AC-SYNC-002 conflict keeps local copy", suite.test_AC_SYNC_002_ActionConflictKeepsLocalCopy),
    ("AC-ATT-001 screenshot uploads ready", suite.test_AC_ATT_001_ScreenshotUploadsAndBecomesReady),
    ("AC-SYNC-003 pull applies remote page", suite.test_AC_SYNC_003_PullAppliesRemotePage),
    ("AC-NOT-001 scheduler due and fired", suite.test_AC_NOT_001_SchedulerDueAndFired),
    ("AC-DOM-003 delete stops reminders", suite.test_AC_DOM_003_DeleteStopsRemindersAndQueuesAction),
    ("AC-CAP-004 ocr body with auto title", suite.test_AC_CAP_004_OCRTextStoredAsBodyWithAutoTitle),
    ("AC-DOM-004 update queues PUT op", suite.test_AC_DOM_004_UpdateQueuesPutOperation),
    ("AC-MAC-001 screenshot top-left conversion", suite.test_AC_MAC_001_ScreenshotUsesTopLeftDisplayCoordinates),
    ("AC-MAC-001 screenshot clamp and pixel alignment", suite.test_AC_MAC_001_ScreenshotClampsAndAlignsToPixels),
    ("AC-MAC-001 screenshot ignores global screen offset", suite.test_AC_MAC_001_ScreenshotDoesNotApplyGlobalScreenOffset)
]
var failures = 0
for (name, run) in cases {
    do { try run(); print("PASS \(name)") }
    catch { failures += 1; print("FAIL \(name): \(error)") }
}
print("\(cases.count - failures)/\(cases.count) passed")
exit(failures == 0 ? 0 : 1)
