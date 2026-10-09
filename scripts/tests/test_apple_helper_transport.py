"""Native Apple helper request-boundary regression tests."""

from __future__ import annotations

import json
import os
import pathlib
import platform
import shutil
import subprocess
import sys
import tempfile
import unittest


REPO = pathlib.Path(__file__).resolve().parents[2]
HELPER_SOURCE = REPO / "apps/heiwa_app/desktop/native/AppleResources.swift"
MAX_REQUEST_BYTES = 4 * 1024 * 1024
PRIVATE_REQUEST = json.dumps(
    {"operation": "list", "private": "PRIVATE_SENTINEL"}, separators=(",", ":")
)


@unittest.skipUnless(
    sys.platform == "darwin",
    "unsupported: Apple EventKit helper transport requires macOS",
)
class AppleHelperTransportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        xcrun = shutil.which("xcrun")
        if xcrun is None:
            raise RuntimeError("required native helper coverage unavailable: xcrun is missing")
        sdk = subprocess.check_output(
            [xcrun, "--sdk", "macosx", "--show-sdk-path"], text=True, timeout=15
        ).strip()
        if not (pathlib.Path(sdk) / "System/Library/Frameworks/EventKit.framework").exists():
            raise RuntimeError("required native helper coverage unavailable: EventKit SDK is missing")

        cls._temporary = tempfile.TemporaryDirectory(prefix="heiwa-apple-helper-tests-")
        cls.addClassCleanup(cls._temporary.cleanup)
        temporary = pathlib.Path(cls._temporary.name)
        source = HELPER_SOURCE.read_text()
        harness_source = cls._reader_harness_source(source)
        harness_path = temporary / "AppleHelperTransportHarness.swift"
        harness_path.write_text(harness_source)
        cls.harness = temporary / "apple-helper-transport-harness"
        cls.helper = temporary / "heiwa-apple-resources"
        cls.reminders_harness = temporary / "apple-reminders-harness"
        cls.cancellation_harness = temporary / "apple-reminders-cancellation-harness"
        cls.fixture = temporary / "reminders-fixture.json"
        cls.trace = temporary / "reminders-trace.txt"
        target = f"{platform.machine()}-apple-macosx27.0"
        cls._compile(harness_path, cls.harness, target)
        cls._compile(HELPER_SOURCE, cls.helper, target)
        reminders_path = temporary / "AppleRemindersHarness.swift"
        reminders_path.write_text(source.replace("import EventKit", FAKE_EVENTKIT, 1))
        cls._compile(reminders_path, cls.reminders_harness, target)
        cancellation_path = temporary / "AppleRemindersCancellationHarness.swift"
        cancellation_source = source.replace("import EventKit", FAKE_EVENTKIT, 1)
        cancellation_source = cancellation_source.replace("@main\nstruct AppleResources {", "struct AppleResources {", 1)
        main_start = cancellation_source.index("    static func main() async {")
        reader_start = cancellation_source.index("    static func readRequest(", main_start)
        cancellation_source = cancellation_source[:main_start] + cancellation_source[reader_start:]
        cancellation_path.write_text(cancellation_source + CANCELLATION_MAIN)
        cls._compile(cancellation_path, cls.cancellation_harness, target)

    @staticmethod
    def _reader_harness_source(source: str) -> str:
        source = source.replace("@main\nstruct AppleResources {", "struct AppleResources {", 1)
        main_start = source.index("    static func main() async {")
        reader_start = source.index("    static func readRequest(", main_start)
        source = source[:main_start] + source[reader_start:]
        return source + r'''

@main struct AppleHelperTransportHarness {
    static func main() {
        let arguments: [String]
        if ProcessInfo.processInfo.environment["HARNESS_OVERSIZED_LEGACY"] == "1" {
            arguments = [String(repeating: "x", count: AppleResources.maximumRequestBytes + 1)]
        } else {
            arguments = Array(CommandLine.arguments.dropFirst())
        }
        do {
            let data = try AppleResources.readRequest(arguments: arguments)
            let expected = Data(ProcessInfo.processInfo.environment["HARNESS_EXPECT"]!.utf8)
            guard data == expected else { fputs("mismatch\n", stderr); exit(3) }
            print("accepted")
        } catch {
            print("rejected")
            exit(1)
        }
    }
}
'''

    @classmethod
    def _compile(cls, source: pathlib.Path, output: pathlib.Path, target: str) -> None:
        subprocess.run(
            [
                "xcrun",
                "swiftc",
                "-parse-as-library",
                "-O",
                "-target",
                target,
                "-framework",
                "EventKit",
                "-framework",
                "Foundation",
                str(source),
                "-o",
                str(output),
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=120,
        )

    def run_harness(
        self,
        payload: bytes,
        arguments: tuple[str, ...] = (),
        *,
        accepted: bool,
        oversized_legacy: bool = False,
    ) -> None:
        env = os.environ.copy()
        env["HARNESS_EXPECT"] = PRIVATE_REQUEST
        if oversized_legacy:
            env["HARNESS_OVERSIZED_LEGACY"] = "1"
        result = subprocess.run(
            [str(self.harness), *arguments],
            input=payload,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            timeout=15,
        )
        self.assertEqual(result.returncode == 0, accepted, result.stderr)
        self.assertEqual(result.stdout.strip(), b"accepted" if accepted else b"rejected")
        self.assertNotIn(b"PRIVATE_SENTINEL", result.stdout + result.stderr)

    def test_actual_reader_prefers_bounded_stdin_and_supports_one_legacy_argument(self) -> None:
        payload = PRIVATE_REQUEST.encode()
        self.run_harness(payload, accepted=True)
        self.run_harness(b"", (PRIVATE_REQUEST,), accepted=True)
        self.run_harness(payload, (PRIVATE_REQUEST,), accepted=False)

    def test_actual_reader_rejects_empty_multiple_and_oversized_requests(self) -> None:
        self.run_harness(b"", accepted=False)
        self.run_harness(b"", ("{}", "{}"), accepted=False)
        self.run_harness(b"x" * (MAX_REQUEST_BYTES + 1), accepted=False)
        # Exercise the same reader with a synthetic argument to avoid macOS ARG_MAX.
        self.run_harness(b"", accepted=False, oversized_legacy=True)

    def test_public_helper_rejects_bad_requests_without_echo_or_calendar_access(self) -> None:
        unsupported = json.dumps(
            {"operation": "unsupported", "private": "PRIVATE_SENTINEL"}
        ).encode()
        cases = [
            (b'{"private":"PRIVATE_SENTINEL"', ()),
            (unsupported, ()),
            (b"", ()),
            (b"x" * (MAX_REQUEST_BYTES + 1), ()),
            (unsupported, (PRIVATE_REQUEST,)),
            (b"", ("{}", "{}")),
        ]
        for payload, arguments in cases:
            with self.subTest(payload_size=len(payload), argument_count=len(arguments)):
                result = subprocess.run(
                    [str(self.helper), *arguments],
                    input=payload,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    timeout=15,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn(b"PRIVATE_SENTINEL", result.stdout + result.stderr)
                response = json.loads(result.stdout)
                self.assertEqual(response["schema_version"], 1)
                self.assertIn("error", response)

    def reminder_request(self, request: dict, fixture: dict | None = None) -> tuple[dict, list[str]]:
        # The complete helper runs against fake SDK types. No real EventKit store,
        # permission request, Apple content, or mutation is involved.
        self.fixture.write_text(json.dumps(fixture or {}))
        self.trace.write_text("")
        env = os.environ.copy()
        env["HARNESS_FIXTURE"] = str(self.fixture)
        env["HARNESS_TRACE"] = str(self.trace)
        result = subprocess.run(
            [str(self.reminders_harness)],
            input=json.dumps(request).encode(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            timeout=15,
        )
        self.assertNotIn(b"PRIVATE_SENTINEL", result.stdout + result.stderr)
        self.assertLessEqual(len(result.stdout), 1024 * 1024)
        response = json.loads(result.stdout)
        self.assertEqual(response["schema_version"], 1)
        self.assertEqual(result.returncode == 0, "error" not in response)
        return response, self.trace.read_text().splitlines()

    def test_reminders_selection_is_validated_before_store_or_permission(self) -> None:
        for ids in (None, [], [""], [" "], ["a", "a"], ["x" * 1025], ["a\nb"], ["a\u0000b"], [str(i) for i in range(101)]):
            with self.subTest(ids=ids):
                response, trace = self.reminder_request(
                    {"operation": "reminders_scan", "list_ids": ids, "limit": 1, "request_access": True}
                )
                self.assertEqual(response["error"], "invalid_request")
                self.assertEqual(trace, [])
        for limit in (None, 0, 501, True, "1", 1.5):
            response, trace = self.reminder_request(
                {"operation": "reminders_scan", "list_ids": ["a"], "limit": limit}
            )
            self.assertEqual(response["error"], "invalid_request")
            self.assertEqual(trace, [])
        response, trace = self.reminder_request(
            {"operation": "reminders_scan", "list_ids": ["missing"], "limit": 1},
            {"lists": [{"id": "a"}]},
        )
        self.assertEqual(response["error"], "selected_reminder_list_unavailable")
        self.assertFalse(any(line.startswith("fetch") for line in trace))

    def test_reminders_read_operations_never_admit_writes(self) -> None:
        for operation in ("reminders_apply", "reminders_save", "reminders_delete", "reminders_stage"):
            response, trace = self.reminder_request({"operation": operation, "request_access": True})
            self.assertEqual(response["error"], "invalid_request")
            self.assertEqual(trace, [])
        response, trace = self.reminder_request({"operation": "reminders_list"})
        self.assertIn("lists", response)
        self.assertFalse(any(line.startswith(("save", "remove", "commit", "reset")) for line in trace))

    def test_reminders_permission_is_separate_and_only_explicit_list_requests_prompt(self) -> None:
        for access in (None, "true", 1, [], {}):
            response, trace = self.reminder_request({"operation": "reminders_list", "request_access": access})
            self.assertEqual(response["error"], "invalid_request")
            self.assertEqual(trace, [])
        fixture = {"event_access": "denied", "reminder_access": "notDetermined", "grant_access": True}
        for request in (
            {"operation": "reminders_list"},
            {"operation": "reminders_scan", "list_ids": ["a"], "limit": 1, "request_access": True},
        ):
            response, trace = self.reminder_request(request, fixture)
            self.assertEqual(response["error"], "reminders_access_required")
            self.assertNotIn("request:reminder", trace)
            self.assertFalse(any("event" in line for line in trace))
        response, trace = self.reminder_request(
            {"operation": "reminders_list", "request_access": True}, fixture
        )
        self.assertIn("lists", response)
        self.assertEqual(trace.count("request:reminder"), 1)
        self.assertFalse(any("event" in line for line in trace))
        response, trace = self.reminder_request({"operation": "list"}, {
            "event_access": "fullAccess", "reminder_access": "denied"
        })
        self.assertIn("calendars", response)
        self.assertFalse(any("reminder" in line for line in trace))
        response, trace = self.reminder_request(
            {"operation": "reminders_list", "request_access": True},
            {"reminder_access": "notDetermined", "grant_access": False},
        )
        self.assertEqual(response["error"], "reminders_access_required")

    def test_reminders_projection_preserves_due_identity_and_filters_private_fields(self) -> None:
        marker = "heiwa://effect/rem-evt-" + "a" * 64
        fixture = {
            "lists": [{"id": "a", "name": "Tasks", "source": "Local", "writable": False}, {"id": "b"}],
            "reminders": [
                {"id": "r1", "list_id": "a", "title": "Date only", "due": {"year": 2026, "month": 10, "day": 9}, "url": marker, "notes": "PRIVATE_SENTINEL"},
                {"id": "r2", "list_id": "a", "title": "Timed", "due": {"year": 2026, "month": 10, "day": 9, "hour": 12, "minute": 30, "time_zone": "America/Vancouver"}, "completed": True, "url": "https://example.test/PRIVATE_SENTINEL"},
                {"id": "r3", "list_id": "a", "title": "No due"},
                {"id": "r4", "list_id": "a", "title": "Floating", "due": {"year": 2026, "month": 10, "day": 9, "hour": 12, "minute": 30}},
                {"id": "r5", "list_id": "a", "title": "Date with zone", "due": {"year": 2026, "month": 10, "day": 9, "time_zone": "Pacific/Kiritimati"}},
                {"id": "hidden", "list_id": "b", "title": "PRIVATE_SENTINEL"},
            ],
            "current_zone": "America/Vancouver",
        }
        response, trace = self.reminder_request({"operation": "reminders_list"}, fixture)
        self.assertEqual(response["lists"][0], {"id": "a", "name": "Tasks", "source": "Local", "writable": False})
        self.assertFalse(response["truncated"])
        response, trace = self.reminder_request(
            {"operation": "reminders_scan", "list_ids": ["a"], "limit": 500}, fixture
        )
        self.assertEqual(response["list_ids"], ["a"])
        rows = response["reminders"]
        self.assertEqual(len(rows), 5)
        self.assertEqual(rows[0]["marker"], marker)
        self.assertEqual(rows[0]["due"], {"date": "2026-10-09"})
        self.assertEqual(rows[1]["due"], {"instant": "2026-10-09T19:30:00Z"})
        self.assertTrue(rows[1]["completed"])
        self.assertIsNone(rows[1]["marker"])
        self.assertIsNone(rows[2]["due"])
        self.assertIsNone(rows[2]["marker"])
        self.assertEqual(rows[3]["due"], {"instant": "2026-10-09T19:30:00Z"})
        self.assertEqual(rows[4]["due"], {"date": "2026-10-09"})
        self.assertTrue(all(set(row) == {"id", "list_id", "title", "due", "completed", "marker"} for row in rows))
        self.assertIn("predicate:a", trace)

    def test_reminders_partial_or_invalid_evidence_is_never_reported_complete(self) -> None:
        request = {"operation": "reminders_scan", "list_ids": ["a"], "limit": 1}
        for due in (
            {}, {"year": 2026, "month": 2, "day": 30},
            {"year": 2026, "month": 10, "day": 9, "minute": 30},
            {"year": 2026, "month": 3, "day": 8, "hour": 2, "minute": 30, "time_zone": "America/Vancouver"},
        ):
            response, _ = self.reminder_request(request, {
                "lists": [{"id": "a"}], "reminders": [{"id": "r", "list_id": "a", "due": due}]
            })
            self.assertEqual(response["error"], "reminders_read_failed")
        marker = "heiwa://effect/rem-evt-" + "a" * 64
        for url in (marker + "?private=x", marker + "#x", marker.upper(), marker[:-1], marker.replace("effect/", "user@effect/")):
            response, _ = self.reminder_request(request, {
                "lists": [{"id": "a"}], "reminders": [{"id": "r", "list_id": "a", "url": url}]
            })
            self.assertEqual(response["error"], "reminders_read_failed")
        for mode in ("nil", "timeout"):
            response, trace = self.reminder_request(request, {"lists": [{"id": "a"}], "fetch": mode})
            self.assertEqual(response["error"], "reminders_read_failed")
            if mode == "timeout":
                self.assertIn("cancel", trace)

    def test_reminders_fetch_cancellation_finishes_without_waiting_for_a_callback(self) -> None:
        self.fixture.write_text(json.dumps({"fetch": "timeout"}))
        self.trace.write_text("")
        env = os.environ.copy()
        env["HARNESS_FIXTURE"] = str(self.fixture)
        env["HARNESS_TRACE"] = str(self.trace)
        result = subprocess.run(
            [str(self.cancellation_harness)], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env=env, timeout=2,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), b"reminders_read_failed")
        self.assertEqual(self.trace.read_text().splitlines().count("cancel"), 1)

    def test_reminders_inventory_rows_and_response_bytes_are_bounded_without_text_clipping(self) -> None:
        response, _ = self.reminder_request({"operation": "reminders_list"}, {
            "lists": [{"id": str(i)} for i in range(101)]
        })
        self.assertEqual(len(response["lists"]), 100)
        self.assertTrue(response["truncated"])
        response, _ = self.reminder_request(
            {"operation": "reminders_scan", "list_ids": ["a"], "limit": 1}, {
                "lists": [{"id": "a"}], "reminders": [
                    {"id": "r1", "list_id": "a", "title": "a" * 5000},
                    {"id": "r2", "list_id": "a"},
                ]
            }
        )
        self.assertEqual(response["reminders"][0]["title"], "a" * 5000)
        self.assertTrue(response["truncated"])
        response, _ = self.reminder_request(
            {"operation": "reminders_scan", "list_ids": ["a"], "limit": 500}, {
                "lists": [{"id": "a"}], "reminders": [{"id": "r", "list_id": "a", "title": "a" * (1024 * 1024)}]
            }
        )
        self.assertEqual(response["reminders"], [])
        self.assertTrue(response["truncated"])
        for key in ("id", "name", "source"):
            row = {"id": "a", key: "a" * (1024 * 1024)}
            response, _ = self.reminder_request({"operation": "reminders_list"}, {"lists": [row]})
            self.assertEqual(response["lists"], [])
            self.assertTrue(response["truncated"])

    def test_reminders_projection_uses_the_cli_field_limits_and_never_clips_identity(self) -> None:
        for key, count in (("id", 1025), ("name", 16 * 1024 + 1), ("source", 16 * 1024 + 1)):
            response, _ = self.reminder_request({"operation": "reminders_list"}, {
                "lists": [{"id": "a", key: "é" * count}]
            })
            self.assertEqual(response["lists"], [])
            self.assertTrue(response["truncated"])
        for key, count in (("id", 1025), ("title", 16 * 1024 + 1)):
            response, _ = self.reminder_request(
                {"operation": "reminders_scan", "list_ids": ["a"], "limit": 500}, {
                    "lists": [{"id": "a"}], "reminders": [{"id": "r", "list_id": "a", key: "x" * count}]
                }
            )
            self.assertEqual(response["reminders"], [])
            self.assertTrue(response["truncated"])
        response, _ = self.reminder_request(
            {"operation": "reminders_scan", "list_ids": ["a"], "limit": 500}, {
                "lists": [{"id": "a"}], "reminders": [
                    {"id": str(i), "list_id": "a", "title": "x" * (16 * 1024)} for i in range(100)
                ]
            }
        )
        self.assertTrue(response["truncated"])
        self.assertGreater(len(response["reminders"]), 0)
        self.assertLess(len(response["reminders"]), 100)
        self.assertTrue(all(row["title"] == "x" * (16 * 1024) for row in response["reminders"]))


FAKE_EVENTKIT = r'''
// Hermetic EventKit-shaped SDK. These types cannot reach an Apple resource.
enum EKEntityType { case event, reminder }
enum EKAuthorizationStatus: String { case notDetermined, fullAccess, writeOnly, denied, restricted }
enum EKEventStatus { case confirmed, canceled }
enum EKEventAvailability { case tentative, busy }
enum EKSpan { case thisEvent }
enum FixtureError: Error { case failed }

func fixture() -> [String: Any] {
    let data = try! Data(contentsOf: URL(fileURLWithPath: ProcessInfo.processInfo.environment["HARNESS_FIXTURE"]!))
    return try! JSONSerialization.jsonObject(with: data) as! [String: Any]
}
func trace(_ value: String) {
    let file = FileHandle(forWritingAtPath: ProcessInfo.processInfo.environment["HARNESS_TRACE"]!)!
    file.seekToEndOfFile()
    file.write(Data((value + "\n").utf8))
    try! file.close()
}
class EKSource { var title = "Local" }
class EKCalendar {
    var calendarIdentifier: String
    var title: String
    var source = EKSource()
    var allowsContentModifications: Bool
    var supportedEventAvailabilities: Set<EKEventAvailability> = [.tentative, .busy]
    init(_ row: [String: Any]) {
        calendarIdentifier = row["id"] as? String ?? "a"
        title = row["name"] as? String ?? "Tasks"
        source.title = row["source"] as? String ?? "Local"
        allowsContentModifications = row["writable"] as? Bool ?? true
    }
}
class EKEvent {
    var eventIdentifier: String? = "e"
    var calendar = EKCalendar([:])
    var title: String? = "Event"
    var startDate = Date(timeIntervalSince1970: 0)
    var endDate = Date(timeIntervalSince1970: 3600)
    var occurrenceDate: Date? = nil
    var hasRecurrenceRules = false
    var isDetached = false
    var isAllDay = false
    var status = EKEventStatus.confirmed
    var availability = EKEventAvailability.busy
    var notes: String? = nil
    var location: String? = nil
    var url: URL? = nil
    init(eventStore: EKEventStore) { trace("new:event") }
}
class EKReminder {
    var calendarItemIdentifier: String
    var calendar: EKCalendar
    var title: String?
    var dueDateComponents: DateComponents?
    var isCompleted: Bool
    var url: URL?
    init(_ row: [String: Any], calendars: [EKCalendar]) {
        calendarItemIdentifier = row["id"] as? String ?? "r"
        let listID = row["list_id"] as? String ?? "a"
        calendar = calendars.first { $0.calendarIdentifier == listID } ?? EKCalendar(["id": listID])
        title = row["title"] as? String ?? "Reminder"
        isCompleted = row["completed"] as? Bool ?? false
        url = (row["url"] as? String).flatMap { URL(string: $0) }
        if let due = row["due"] as? [String: Any] {
            var components = DateComponents()
            components.year = due["year"] as? Int
            components.month = due["month"] as? Int
            components.day = due["day"] as? Int
            components.hour = due["hour"] as? Int
            components.minute = due["minute"] as? Int
            components.second = due["second"] as? Int
            components.nanosecond = due["nanosecond"] as? Int
            components.timeZone = (due["time_zone"] as? String).flatMap { TimeZone(identifier: $0) }
            dueDateComponents = components
        }
    }
}
class EKEventStore {
    static var grantedReminders = false
    static func authorizationStatus(for entity: EKEntityType) -> EKAuthorizationStatus {
        let kind = entity == .event ? "event" : "reminder"
        trace("authorization:" + kind)
        if entity == .reminder && grantedReminders { return .fullAccess }
        return EKAuthorizationStatus(rawValue: fixture()[kind + "_access"] as? String ?? "fullAccess")!
    }
    init() {
        trace("store")
        if let zone = fixture()["current_zone"] as? String { NSTimeZone.default = TimeZone(identifier: zone)! }
    }
    func requestFullAccessToEvents() async throws -> Bool { trace("request:event"); return true }
    func requestFullAccessToReminders() async throws -> Bool {
        trace("request:reminder")
        Self.grantedReminders = fixture()["grant_access"] as? Bool ?? false
        return Self.grantedReminders
    }
    func calendars(for entity: EKEntityType) -> [EKCalendar] {
        trace(entity == .event ? "calendars:event" : "calendars:reminder")
        return (fixture()["lists"] as? [[String: Any]] ?? []).map { EKCalendar($0) }
    }
    func predicateForReminders(in calendars: [EKCalendar]?) -> NSPredicate {
        let ids = calendars?.map { $0.calendarIdentifier }
        trace("predicate:" + (ids?.joined(separator: ",") ?? "WILDCARD"))
        return NSPredicate { object, _ in
            guard let reminder = object as? EKReminder else { return false }
            return ids?.contains(reminder.calendar.calendarIdentifier) ?? true
        }
    }
    func fetchReminders(matching predicate: NSPredicate, completion: @escaping ([EKReminder]?) -> Void) -> Any {
        trace("fetch")
        let mode = fixture()["fetch"] as? String ?? "success"
        if mode != "timeout" {
            let allCalendars = (fixture()["lists"] as? [[String: Any]] ?? []).map { EKCalendar($0) }
            let rows = (fixture()["reminders"] as? [[String: Any]] ?? []).map { EKReminder($0, calendars: allCalendars) }
            completion(mode == "nil" ? nil : rows.filter { predicate.evaluate(with: $0) })
        }
        return NSObject()
    }
    func cancelFetchRequest(_ token: Any) { trace("cancel") }
    func predicateForEvents(withStart start: Date, end: Date, calendars: [EKCalendar]?) -> NSPredicate { NSPredicate(value: false) }
    func events(matching predicate: NSPredicate) -> [EKEvent] { [] }
    func event(withIdentifier id: String) -> EKEvent? { nil }
    func remove(_ event: EKEvent, span: EKSpan, commit: Bool) throws { trace("remove:event") }
    func save(_ event: EKEvent, span: EKSpan, commit: Bool) throws { trace("save:event") }
    func commit() throws { trace("commit") }
    func reset() { trace("reset") }
}
'''

CANCELLATION_MAIN = r'''
@main struct CancellationHarness {
    static func main() async {
        let store = EKEventStore()
        let task = Task { try await AppleResources.fetchReminders(store: store, predicate: NSPredicate(value: true)) }
        try! await Task.sleep(for: .milliseconds(50))
        task.cancel()
        do { _ = try await task.value; exit(2) }
        catch { print((error as? AppleResources.ReadError)?.rawValue ?? "unexpected"); }
    }
}
'''


if __name__ == "__main__":
    unittest.main()
