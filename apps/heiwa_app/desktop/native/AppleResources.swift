import Foundation
import CoreFoundation
import EventKit

// Protocol data only. Rust owns enrollment, selection, persistence, and effects.
@main
struct AppleResources {
    static let maximumRequestBytes = 4 * 1024 * 1024
    static let maximumReminderResponseBytes = 1024 * 1024
    static let maximumReminderTextBytes = 16 * 1024
    static let maximumResourceIdentifierBytes = 1024
    static let reminderFetchTimeout: TimeInterval = 5

    static func main() async {
        var readFailure = "calendar_read_failed"
        do {
            let data = try readRequest()
            guard let request = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let operation = request["operation"] as? String,
                  ["list", "scan", "plan_scan", "plan_apply", "reminders_list", "reminders_scan"].contains(operation) else { throw ReadError.invalidRequest }
            try validateSchemaVersion(request)
            let calendarScan = operation == "scan" ? try calendarSelection(request) : nil
            let requestAccess = ["list", "reminders_list"].contains(operation) ? try resourceAccessRequested(request) : false
            if operation == "reminders_scan" {
                _ = try reminderSelection(request)
            }
            let store = EKEventStore()
            if operation == "reminders_list" || operation == "reminders_scan" {
                readFailure = ReadError.remindersReadFailed.rawValue
                output(try await remindersRead(store: store, request: request, operation: operation))
                return
            }
            if [.notDetermined, .writeOnly].contains(EKEventStore.authorizationStatus(for: .event)) {
                guard operation == "list", requestAccess else { throw ReadError.permission }
                guard try await store.requestFullAccessToEvents() else { throw ReadError.permission }
            }
            guard EKEventStore.authorizationStatus(for: .event) == .fullAccess else { throw ReadError.permission }
            let calendars = store.calendars(for: .event)
            if operation == "plan_scan" {
                try planScan(store: store, calendars: calendars, request: request)
                return
            }
            if operation == "plan_apply" {
                try planApply(store: store, calendars: calendars, request: request)
                return
            }
            if operation == "list" {
                output(["schema_version": 1, "calendars": calendars.map { calendar in
                    ["id": calendar.calendarIdentifier, "name": calendar.title,
                     "source": calendar.source.title, "writable": calendar.allowsContentModifications] as [String: Any]
                }])
                return
            }
            let iso = ISO8601DateFormatter()
            guard let scan = calendarScan else { throw ReadError.invalidRequest }
            let (ids, startText, endText, start, end, limit) = (scan.ids, scan.startText, scan.endText, scan.start, scan.end, scan.limit)
            let selected = calendars.filter { ids.contains($0.calendarIdentifier) }
            guard selected.count == ids.count else { throw ReadError.missingCalendar }
            // EventKit expands recurrence occurrences within the predicate's range.
            let events = store.events(matching: store.predicateForEvents(withStart: start, end: end, calendars: selected))
                .sorted { ($0.startDate, $0.eventIdentifier ?? "") < ($1.startDate, $1.eventIdentifier ?? "") }
            let day = DateFormatter()
            day.locale = Locale(identifier: "en_US_POSIX")
            day.calendar = Calendar(identifier: .gregorian)
            day.timeZone = .current
            day.dateFormat = "yyyy-MM-dd"
            let rows: [[String: Any]] = try events.prefix(limit).map { event in
                guard let identifier = event.eventIdentifier, !identifier.isEmpty else { throw ReadError.invalidEvent }
                return ["source": "apple_calendar", "calendar_id": event.calendar.calendarIdentifier,
                        "calendar": event.calendar.title, "external_id": identifier,
                        "occurrence": event.hasRecurrenceRules || event.isDetached ? iso.string(from: event.occurrenceDate ?? event.startDate) : "",
                        "title": String((event.title ?? "Untitled").prefix(4096)),
                        "start": iso.string(from: event.startDate), "end": iso.string(from: event.endDate),
                        "date": day.string(from: event.startDate), "all_day": event.isAllDay,
                        "status": event.status == .canceled ? "cancelled" : "confirmed"]
            }
            output(["schema_version": 1, "calendar_ids": ids, "start": startText, "end": endText,
                    "events": rows, "truncated": events.count > limit])
        } catch {
            // Never serialize an EventKit exception or user calendar contents.
            output(["schema_version": 1, "error": (error as? ReadError)?.rawValue ?? readFailure])
            exit(1)
        }
    }

    static func readRequest(
        arguments: [String] = Array(CommandLine.arguments.dropFirst())
    ) throws -> Data {
        var data = Data()
        while data.count <= maximumRequestBytes {
            let remaining = maximumRequestBytes + 1 - data.count
            let chunk = FileHandle.standardInput.readData(ofLength: min(64 * 1024, remaining))
            if chunk.isEmpty { break }
            data.append(chunk)
        }
        guard data.count <= maximumRequestBytes else { throw ReadError.invalidRequest }
        if !data.isEmpty {
            // The new runtime always uses stdin. Reject mixed/extra arguments;
            // only the old released CLI used one JSON argument during upgrade.
            guard arguments.isEmpty else { throw ReadError.invalidRequest }
            return data
        }
        return try legacyRequest(arguments)
    }

    static func legacyRequest(_ arguments: [String]) throws -> Data {
        guard arguments.count == 1 else { throw ReadError.invalidRequest }
        let legacyData = Data(arguments[0].utf8)
        guard !legacyData.isEmpty, legacyData.count <= maximumRequestBytes else {
            throw ReadError.invalidRequest
        }
        return legacyData
    }

    // MARK: Pure request validation

    static func boundedInteger(_ value: Any?, in bounds: ClosedRange<Int>) throws -> Int {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue >= Double(bounds.lowerBound), number.doubleValue <= Double(bounds.upperBound),
              number.doubleValue == Double(number.intValue) else { throw ReadError.invalidRequest }
        return number.intValue
    }

    static func validateSchemaVersion(_ request: [String: Any]) throws {
        // Older Rust/helper pairs omitted this field. A provided version is
        // an explicit contract and must not silently select an older protocol.
        if let version = request["schema_version"] {
            _ = try boundedInteger(version, in: 1...1)
        }
    }

    static func resourceAccessRequested(_ request: [String: Any]) throws -> Bool {
        guard let value = request["request_access"] else { return false }
        guard let number = value as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID() else { throw ReadError.invalidRequest }
        return number.boolValue
    }

    static func resourceIdentifierValid(_ id: String) -> Bool {
        !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
            id.utf8.count <= maximumResourceIdentifierBytes &&
            !id.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    struct CalendarScanRequest {
        let ids: [String]
        let startText: String
        let endText: String
        let start: Date
        let end: Date
        let limit: Int
    }

    static func calendarInstant(_ value: Any?, iso: ISO8601DateFormatter) throws -> (String, Date) {
        guard let text = value as? String, (20...25).contains(text.utf8.count),
              let date = iso.date(from: text) else { throw ReadError.invalidRequest }
        // Foundation accepts and normalizes impossible days (e.g. February
        // 30). Round-trip the civil components separately so offsets remain
        // accepted while invalid dates cannot change the requested window.
        let civilText = String(text.prefix(19)) + "Z"
        let zone = String(text.dropFirst(19))
        guard zone.range(of: "^(Z|[+-][0-9]{2}:?[0-9]{2})$", options: .regularExpression) != nil,
              let civil = iso.date(from: civilText), iso.string(from: civil) == civilText else { throw ReadError.invalidRequest }
        if zone != "Z" {
            let digits = zone.dropFirst().replacingOccurrences(of: ":", with: "")
            guard let hours = Int(digits.prefix(2)), hours <= 23,
                  let minutes = Int(digits.suffix(2)), minutes <= 59 else { throw ReadError.invalidRequest }
        }
        return (text, date)
    }

    static func calendarSelection(_ request: [String: Any]) throws -> CalendarScanRequest {
        guard let ids = request["calendar_ids"] as? [String], !ids.isEmpty, ids.count <= 100,
              Set(ids).count == ids.count, ids.allSatisfy(resourceIdentifierValid) else { throw ReadError.invalidRequest }
        let iso = ISO8601DateFormatter()
        let (startText, start) = try calendarInstant(request["start"], iso: iso)
        let (endText, end) = try calendarInstant(request["end"], iso: iso)
        guard end > start, end.timeIntervalSince(start) <= 366 * 86400 else { throw ReadError.invalidRequest }
        return CalendarScanRequest(ids: ids, startText: startText, endText: endText, start: start, end: end,
                                   limit: try boundedInteger(request["limit"], in: 1...500))
    }

    // MARK: Reminders read-only projection

    static func reminderSelection(_ request: [String: Any]) throws -> ([String], Int) {
        guard let ids = request["list_ids"] as? [String], !ids.isEmpty, ids.count <= 100,
              Set(ids).count == ids.count,
              ids.allSatisfy(resourceIdentifierValid) else {
            throw ReadError.invalidRequest
        }
        return (ids, try boundedInteger(request["limit"], in: 1...500))
    }

    static func remindersRead(store: EKEventStore, request: [String: Any], operation: String) async throws -> [String: Any] {
        // This gate is independent of Calendar enrollment, selection, and TCC.
        let status = EKEventStore.authorizationStatus(for: .reminder)
        if [.notDetermined, .writeOnly].contains(status) {
            guard operation == "reminders_list", try resourceAccessRequested(request) else {
                throw ReadError.remindersPermission
            }
            guard try await store.requestFullAccessToReminders() else { throw ReadError.remindersPermission }
        }
        guard EKEventStore.authorizationStatus(for: .reminder) == .fullAccess else {
            throw ReadError.remindersPermission
        }
        let lists = store.calendars(for: .reminder)
        if operation == "reminders_list" {
            var response = BoundedReminderResponse(key: "lists", envelope: ["schema_version": 1])
            for list in lists.prefix(100) {
                try response.append(["id": list.calendarIdentifier, "name": list.title,
                                     "source": list.source.title, "writable": list.allowsContentModifications])
            }
            return response.value(truncated: lists.count > 100)
        }
        let (ids, limit) = try reminderSelection(request)
        let selected = lists.filter { ids.contains($0.calendarIdentifier) }
        guard selected.count == ids.count, Set(selected.map { $0.calendarIdentifier }) == Set(ids) else {
            throw ReadError.missingReminderList
        }
        // Never pass nil or an empty selection: EventKit treats nil as all lists.
        let predicate = store.predicateForReminders(in: selected)
        // EventKit has no query row cap and may materialize the entire matching
        // backend array. Bound the fetch wait and the projected rows/bytes below.
        let reminders = try await fetchReminders(store: store, predicate: predicate)
        var response = BoundedReminderResponse(key: "reminders", envelope: ["schema_version": 1, "list_ids": ids])
        var seen: Set<String> = []
        for reminder in reminders.prefix(limit) {
            let id = reminder.calendarItemIdentifier
            let listID = reminder.calendar.calendarIdentifier
            guard !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  ids.contains(listID), seen.insert(id).inserted else { throw ReadError.remindersReadFailed }
            let row: [String: Any] = ["id": id, "list_id": listID, "title": reminder.title ?? "",
                                      "due": try reminderDue(reminder.dueDateComponents),
                                      "completed": reminder.isCompleted,
                                      "marker": try reminderMarker(reminder.url)]
            try response.append(row)
        }
        return response.value(truncated: reminders.count > limit)
    }

    // Full matching fields are retained or the row is omitted and the response
    // is explicitly partial. Clipped titles could falsely imply an in-sync effect.
    struct BoundedReminderResponse {
        let key: String
        let envelope: [String: Any]
        var rows: [[String: Any]] = []
        var rowBytes = 0
        var omitted = false

        mutating func append(_ row: [String: Any]) throws {
            if row.contains(where: { key, value in
                guard let text = value as? String else { return false }
                return ["id", "list_id"].contains(key) ? !resourceIdentifierValid(text) : text.utf8.count > maximumReminderTextBytes
            }) {
                omitted = true
                return
            }
            let bytes = try JSONSerialization.data(withJSONObject: row, options: [.sortedKeys]).count
            var base = envelope
            base[key] = [] as [[String: Any]]
            base["truncated"] = false
            let overhead = try JSONSerialization.data(withJSONObject: base, options: [.sortedKeys]).count
            let addition = bytes + (rows.isEmpty ? 0 : 1)
            guard overhead + rowBytes + addition <= maximumReminderResponseBytes else {
                omitted = true
                return
            }
            rows.append(row)
            rowBytes += addition
        }

        func value(truncated: Bool) -> [String: Any] {
            var result = envelope
            result[key] = rows
            result["truncated"] = truncated || omitted
            return result
        }
    }

    static func reminderMarker(_ url: URL?) throws -> Any {
        guard let url else { return NSNull() }
        let text = url.absoluteString
        guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              parts.scheme?.lowercased() == "heiwa", parts.host?.lowercased() == "effect" else {
            return NSNull()
        }
        let prefix = "heiwa://effect/rem-evt-"
        guard text.hasPrefix(prefix), text.utf8.count == prefix.utf8.count + 64,
              parts.user == nil, parts.password == nil, parts.port == nil,
              parts.query == nil, parts.fragment == nil,
              text.dropFirst(prefix.count).utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
            throw ReadError.remindersReadFailed
        }
        return text
    }

    static func reminderDue(_ components: DateComponents?) throws -> Any {
        guard let components else { return NSNull() }
        guard let year = components.year, (1...9999).contains(year),
              let month = components.month, (1...12).contains(month),
              let day = components.day, (1...31).contains(day),
              components.calendar == nil || components.calendar?.identifier == .gregorian,
              components.era == nil || components.era == 1,
              components.isLeapMonth != true,
              components.nanosecond == nil || components.nanosecond == 0 else { throw ReadError.remindersReadFailed }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = components.timeZone ?? components.calendar?.timeZone ?? .current
        let timed = components.hour != nil || components.minute != nil || components.second != nil
        var normalized = DateComponents(year: year, month: month, day: day)
        if timed {
            guard let hour = components.hour, (0...23).contains(hour),
                  let minute = components.minute, (0...59).contains(minute),
                  (0...59).contains(components.second ?? 0) else { throw ReadError.remindersReadFailed }
            normalized.hour = hour
            normalized.minute = minute
            normalized.second = components.second ?? 0
        }
        guard let date = calendar.date(from: normalized) else { throw ReadError.remindersReadFailed }
        // Calendar.date normalizes invalid days and nonexistent DST wall times.
        // A round-trip detects that loss rather than substituting a different due.
        let check = calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: date)
        guard check.year == year, check.month == month, check.day == day,
              !timed || (check.hour == normalized.hour && check.minute == normalized.minute && check.second == normalized.second) else {
            throw ReadError.remindersReadFailed
        }
        let metadata: Set<Calendar.Component> = [.weekday, .weekdayOrdinal, .quarter, .weekOfMonth, .weekOfYear, .yearForWeekOfYear]
        let extra = calendar.dateComponents(metadata, from: date)
        guard metadata.allSatisfy({ components.value(for: $0) == nil || components.value(for: $0) == extra.value(for: $0) }) else {
            throw ReadError.remindersReadFailed
        }
        if timed {
            let iso = ISO8601DateFormatter()
            iso.timeZone = TimeZone(secondsFromGMT: 0)
            return ["instant": iso.string(from: date)]
        }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"
        return ["date": formatter.string(from: date)]
    }

    static func fetchReminders(store: EKEventStore, predicate: NSPredicate) async throws -> [EKReminder] {
        let pending = ReminderFetch(store: store)
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard pending.install(continuation) else { return }
                let timer = DispatchWorkItem { pending.finish(.failure(ReadError.remindersReadFailed), cancel: true) }
                pending.install(timer)
                DispatchQueue.global().asyncAfter(deadline: .now() + reminderFetchTimeout, execute: timer)
                let identifier = store.fetchReminders(matching: predicate) { reminders in
                    if let reminders { pending.finish(.success(reminders), cancel: false) }
                    else { pending.finish(.failure(ReadError.remindersReadFailed), cancel: false) }
                }
                pending.install(identifier: identifier)
            }
        } onCancel: {
            pending.finish(.failure(ReadError.remindersReadFailed), cancel: true)
        }
    }

    // EventKit cancellation suppresses its callback. Resolve the continuation
    // ourselves, once, including timeout/cancellation before token registration.
    final class ReminderFetch: @unchecked Sendable {
        let store: EKEventStore
        let lock = NSLock()
        var continuation: CheckedContinuation<[EKReminder], Error>?
        var identifier: Any?
        var timer: DispatchWorkItem?
        var finished = false
        var cancelled = false

        init(store: EKEventStore) { self.store = store }

        func install(_ continuation: CheckedContinuation<[EKReminder], Error>) -> Bool {
            lock.lock()
            if finished {
                lock.unlock()
                continuation.resume(throwing: ReadError.remindersReadFailed)
                return false
            }
            self.continuation = continuation
            lock.unlock()
            return true
        }

        func install(_ timer: DispatchWorkItem) {
            lock.lock()
            if finished { timer.cancel() } else { self.timer = timer }
            lock.unlock()
        }

        func install(identifier: Any) {
            lock.lock()
            let cancel = finished && cancelled
            if !finished { self.identifier = identifier }
            lock.unlock()
            if cancel { store.cancelFetchRequest(identifier) }
        }

        func finish(_ result: Result<[EKReminder], Error>, cancel: Bool) {
            lock.lock()
            guard !finished else { lock.unlock(); return }
            finished = true
            cancelled = cancel
            let continuation = self.continuation
            self.continuation = nil
            let identifier = self.identifier
            self.identifier = nil
            timer?.cancel()
            timer = nil
            lock.unlock()
            if cancel, let identifier { store.cancelFetchRequest(identifier) }
            continuation?.resume(with: result)
        }
    }

    // MARK: plan sync
    //
    // A plan owns only events whose URL carries its marker
    // (heiwa://calendar/plan/<plan_id>/<key>). Rust computes the diff and the
    // approval; this helper reads the owned events and applies an approved
    // change set in one EventKit commit, so a failure writes nothing.

    static let planMarkerRoot = "heiwa://calendar/plan/"

    static func planCalendar(named name: String, in calendars: [EKCalendar]) throws -> EKCalendar {
        let matches = calendars.filter { $0.title == name }
        guard matches.count == 1 else { throw ReadError.missingCalendar }
        guard matches[0].allowsContentModifications else { throw ReadError.readOnlyCalendar }
        return matches[0]
    }

    static func planRow(_ event: EKEvent, marker: String, iso: ISO8601DateFormatter) throws -> [String: Any] {
        guard let identifier = event.eventIdentifier, !identifier.isEmpty else { throw ReadError.invalidEvent }
        return ["marker": marker, "external_id": identifier, "calendar": event.calendar.title,
                "title": event.title ?? "", "start": iso.string(from: event.startDate),
                "end": iso.string(from: event.endDate), "notes": event.notes ?? "",
                "location": event.location ?? "", "tentative": event.availability == .tentative]
    }

    static func planScan(store: EKEventStore, calendars: [EKCalendar], request: [String: Any]) throws {
        let iso = ISO8601DateFormatter()
        guard let names = request["calendars"] as? [String], !names.isEmpty, names.count <= 50,
              Set(names).count == names.count,
              let prefix = request["marker_prefix"] as? String, prefix.hasPrefix(planMarkerRoot), prefix.hasSuffix("/"),
              let startText = request["start"] as? String, let start = iso.date(from: startText),
              let endText = request["end"] as? String, let end = iso.date(from: endText),
              end > start, end.timeIntervalSince(start) <= 400 * 86400 else { throw ReadError.invalidRequest }
        let adopt = request["adopt"] as? Bool ?? false
        let selected = try names.map { try planCalendar(named: $0, in: calendars) }
        var marked: [[String: Any]] = []
        var unmarked: [[String: Any]] = []
        for event in store.events(matching: store.predicateForEvents(withStart: start, end: end, calendars: selected)) {
            // Plans own single events only; recurring series stay the user's.
            if event.hasRecurrenceRules || event.isAllDay { continue }
            let url = event.url?.absoluteString ?? ""
            if url.hasPrefix(prefix) {
                marked.append(try planRow(event, marker: url, iso: iso))
            } else if adopt && url.isEmpty {
                unmarked.append(try planRow(event, marker: "", iso: iso))
            }
        }
        output(["schema_version": 1, "marked": marked, "unmarked": unmarked])
    }

    static func planApply(store: EKEventStore, calendars: [EKCalendar], request: [String: Any]) throws {
        let iso = ISO8601DateFormatter()
        guard let changes = request["changes"] as? [[String: Any]], !changes.isEmpty, changes.count <= 2000 else {
            throw ReadError.invalidRequest
        }
        var saved: [(EKEvent, String, String)] = []
        var removed: [[String: Any]] = []
        do {
            for change in changes {
                guard let op = change["op"] as? String, ["create", "update", "delete", "adopt"].contains(op),
                      let marker = change["marker"] as? String, marker.hasPrefix(planMarkerRoot),
                      let markerURL = URL(string: marker) else { throw ReadError.invalidRequest }
                if op == "delete" {
                    guard let id = change["external_id"] as? String, let event = store.event(withIdentifier: id),
                          event.url?.absoluteString == marker else { throw ReadError.staleEvent }
                    try store.remove(event, span: .thisEvent, commit: false)
                    removed.append(["op": op, "marker": marker, "external_id": id])
                    continue
                }
                guard let name = change["calendar"] as? String, let title = change["title"] as? String, !title.isEmpty,
                      let startText = change["start"] as? String, let start = iso.date(from: startText),
                      let endText = change["end"] as? String, let end = iso.date(from: endText), end > start
                else { throw ReadError.invalidRequest }
                let calendar = try planCalendar(named: name, in: calendars)
                let event: EKEvent
                switch op {
                case "create":
                    event = EKEvent(eventStore: store)
                case "update":
                    guard let id = change["external_id"] as? String, let existing = store.event(withIdentifier: id),
                          existing.url?.absoluteString == marker else { throw ReadError.staleEvent }
                    event = existing
                default: // adopt: an unmarked event that exactly matches what the plan wants
                    guard let id = change["external_id"] as? String, let existing = store.event(withIdentifier: id),
                          existing.url == nil else { throw ReadError.staleEvent }
                    event = existing
                }
                event.calendar = calendar
                event.title = title
                event.startDate = start
                event.endDate = end
                let notes = change["notes"] as? String ?? ""
                let location = change["location"] as? String ?? ""
                event.notes = notes.isEmpty ? nil : notes
                event.location = location.isEmpty ? nil : location
                event.url = markerURL
                if calendar.supportedEventAvailabilities.contains(.tentative) {
                    event.availability = (change["tentative"] as? Bool ?? false) ? .tentative : .busy
                }
                try store.save(event, span: .thisEvent, commit: false)
                saved.append((event, op, marker))
            }
            try store.commit()
        } catch let error as ReadError {
            store.reset()
            throw error
        } catch {
            store.reset()
            throw ReadError.writeFailed
        }
        let applied = removed + saved.map { event, op, marker in
            ["op": op, "marker": marker, "external_id": event.eventIdentifier ?? ""] as [String: Any]
        }
        output(["schema_version": 1, "applied": applied])
    }

    static func output(_ value: [String: Any]) {
        if let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
            FileHandle.standardOutput.write(data)
        }
    }
    enum ReadError: String, Error {
        case invalidRequest = "invalid_request"
        case permission = "calendar_access_required"
        case missingCalendar = "selected_calendar_unavailable"
        case invalidEvent = "event_identity_unavailable"
        case readOnlyCalendar = "selected_calendar_read_only"
        case staleEvent = "plan_event_changed"
        case writeFailed = "calendar_write_failed"
        case remindersPermission = "reminders_access_required"
        case missingReminderList = "selected_reminder_list_unavailable"
        case remindersReadFailed = "reminders_read_failed"
    }
}
