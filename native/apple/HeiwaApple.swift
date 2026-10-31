import Foundation
import CoreFoundation
import EventKit

// This helper has no durable authority. The server owns selection, approval and receipts.
// All requests arrive over stdin; never put user contents in argv or diagnostics.
@main struct HeiwaApple {
    enum Failure: String, Error { case invalid, permission, missing, stale, readonly, uncertain }
    static let iso = ISO8601DateFormatter()
    static func output(_ value: Any) {
        if let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
            FileHandle.standardOutput.write(data)
        }
    }
    static func string(_ value: Any?, max: Int = 16000) throws -> String {
        guard let s = value as? String, s.utf8.count <= max else { throw Failure.invalid }; return s
    }
    static func date(_ value: Any?) throws -> Date {
        let s = try string(value, max: 40)
        guard s.range(of:"^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}(Z|[+-]\\d{2}:\\d{2})$",options:.regularExpression) != nil,
              let d = iso.date(from:s), let civil = iso.date(from:String(s.prefix(19))+"Z"),
              iso.string(from:civil) == String(s.prefix(19))+"Z" else { throw Failure.invalid }; return d
    }
    static func canonicalEnd(_ event: EKEvent) -> Date? {
        guard let end = event.endDate else { return nil }
        let parts = Calendar.current.dateComponents([.hour,.minute,.second],from:end)
        // EventKit on macOS 27 exposes the inclusive last second for floating all-day ends.
        if event.isAllDay, parts.hour == 23, parts.minute == 59, parts.second == 59 {
            return Calendar.current.date(byAdding:.day,value:1,to:Calendar.current.startOfDay(for:end))
        }
        return end
    }
    static func row(_ event: EKEvent) throws -> [String: Any] {
        guard let id = event.eventIdentifier, let calendar = event.calendar,
              let start = event.startDate, let end = canonicalEnd(event) else { throw Failure.missing }
        // Recurring occurrences are readable; editing a series needs an explicit span contract.
        return ["app":"calendar", "id":id, "resourceId":calendar.calendarIdentifier,
                "title":String((event.title ?? "").prefix(16000)), "body":String((event.notes ?? "").prefix(16000)),
                "start":iso.string(from:start), "end":iso.string(from:end),
                "completed":false, "allDay":event.isAllDay, "modifiedAt":event.lastModifiedDate.map { iso.string(from:$0) } ?? NSNull(),
                "recurring":event.hasRecurrenceRules || event.isDetached,
                "hasAttendees":event.hasAttendees, "contentTruncated":(event.title ?? "").utf8.count > 16000 || (event.notes ?? "").utf8.count > 16000]
    }
    static func dueValue(_ reminder: EKReminder) -> Any {
        guard let due = reminder.dueDateComponents else { return NSNull() }
        if due.hour == nil, let year = due.year, let month = due.month, let day = due.day { return String(format:"%04d-%02d-%02d",year,month,day) }
        return (due.calendar ?? Calendar.current).date(from:due).map { iso.string(from:$0) } ?? NSNull()
    }
    static func dueComponents(_ value: Any) throws -> DateComponents? {
        if value is NSNull { return nil }
        let raw = try string(value,max:40)
        if raw.count == 10 {
            let instant = try date(raw+"T00:00:00Z")
            var calendar = Calendar(identifier:.gregorian); calendar.timeZone = TimeZone(secondsFromGMT:0)!
            return calendar.dateComponents([.year,.month,.day],from:instant)
        }
        let instant = try date(raw)
        return Calendar.current.dateComponents([.calendar,.timeZone,.year,.month,.day,.hour,.minute,.second],from:instant)
    }
    static func row(_ reminder: EKReminder) throws -> [String: Any] {
        guard let calendar = reminder.calendar else { throw Failure.missing }
        return ["app":"reminders", "id":reminder.calendarItemIdentifier, "resourceId":calendar.calendarIdentifier,
                "title":String((reminder.title ?? "").prefix(16000)), "body":String((reminder.notes ?? "").prefix(16000)),
                "start":NSNull(), "end":NSNull(), "completed":reminder.isCompleted, "due":dueValue(reminder), "priority":reminder.priority,
                "modifiedAt":reminder.lastModifiedDate.map { iso.string(from:$0) } ?? NSNull(),
                "recurring":reminder.hasRecurrenceRules, "hasAttendees":false, "contentTruncated":(reminder.title ?? "").utf8.count > 16000 || (reminder.notes ?? "").utf8.count > 16000]
    }
    static func unchanged(_ actual: [String: Any], _ expected: [String: Any]) throws {
        for key in ["app","id","resourceId","title","body","start","end","completed","modifiedAt","recurring","hasAttendees","contentTruncated","due","priority","allDay"].filter({ actual[$0] != nil }) {
            guard let a = actual[key] as? NSObject, let e = expected[key] as? NSObject, a.isEqual(e) else { throw Failure.stale }
        }
    }
    static func fetch(_ store: EKEventStore, _ calendars: [EKCalendar]) async throws -> [EKReminder] {
        // The parent enforces a hard deadline and kills a stalled EventKit child.
        await withCheckedContinuation { continuation in
            store.fetchReminders(matching:store.predicateForReminders(in:calendars)) { continuation.resume(returning:$0 ?? []) }
        }
    }
    static func main() async {
        do {
            var data = Data()
            while data.count <= 1024*1024 {
                let part = FileHandle.standardInput.readData(ofLength:min(65536,1024*1024+1-data.count))
                if part.isEmpty { break }; data.append(part)
            }
            guard data.count <= 1024*1024, CommandLine.arguments.count == 1,
                  let q = try JSONSerialization.jsonObject(with:data) as? [String:Any],
                  let app = q["app"] as? String, ["calendar","reminders","files"].contains(app),
                  let operation = q["operation"] as? String, ["inventory","read","inspect","create","update","complete","trash"].contains(operation)
            else { throw Failure.invalid }
            if app == "files" { output(try HeiwaFiles.perform(q)); return }
            let store = EKEventStore()
            let entity: EKEntityType = app == "calendar" ? .event : .reminder
            if operation == "inventory", q["requestAccess"] as? Bool == true,
               [.notDetermined,.writeOnly].contains(EKEventStore.authorizationStatus(for:entity)) {
                let allowed = app == "calendar" ? try await store.requestFullAccessToEvents() : try await store.requestFullAccessToReminders()
                guard allowed else { throw Failure.permission }
            }
            guard EKEventStore.authorizationStatus(for:entity) == .fullAccess else { throw Failure.permission }
            let calendars = store.calendars(for:entity)
            if operation == "inventory" {
                output(["resources":calendars.map { ["app":app,"id":$0.calendarIdentifier,"title":$0.title,"source":$0.source.title,"writable":$0.allowsContentModifications] as [String:Any] }]); return
            }
            guard let ids = q["resourceIds"] as? [String], !ids.isEmpty, ids.count <= 100, Set(ids).count == ids.count else { throw Failure.invalid }
            let selected = calendars.filter { ids.contains($0.calendarIdentifier) }
            guard selected.count == ids.count else { throw Failure.missing }
            if operation == "inspect" {
                let id = try string(q["id"], max:1024)
                if app == "calendar" {
                    guard let item = store.event(withIdentifier:id), ids.contains(item.calendar.calendarIdentifier) else { throw Failure.missing }
                    output(["record":try row(item)])
                } else {
                    guard let item = store.calendarItem(withIdentifier:id) as? EKReminder, ids.contains(item.calendar.calendarIdentifier) else { throw Failure.missing }
                    output(["record":try row(item)])
                }
                return
            }
            if operation == "read" {
                guard let number = q["limit"] as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
                      number.intValue >= 1, number.intValue <= 100, number.doubleValue == Double(number.intValue) else { throw Failure.invalid }
                let limit = number.intValue
                let records: [[String:Any]]
                if app == "calendar" {
                    let start = try date(q["start"]), end = try date(q["end"])
                    guard end > start, end.timeIntervalSince(start) <= 31*86400 else { throw Failure.invalid }
                    records = try store.events(matching:store.predicateForEvents(withStart:start,end:end,calendars:selected))
                        .sorted { ($0.startDate,$0.eventIdentifier ?? "") < ($1.startDate,$1.eventIdentifier ?? "") }.prefix(limit+1).map(row)
                } else {
                    records = try await fetch(store,selected).filter { q["includeCompleted"] as? Bool == true || !$0.isCompleted }.sorted {
                        let left = $0.dueDateComponents.flatMap { ($0.calendar ?? Calendar.current).date(from:$0) } ?? Date.distantFuture
                        let right = $1.dueDateComponents.flatMap { ($0.calendar ?? Calendar.current).date(from:$0) } ?? Date.distantFuture
                        return (left,$0.calendarItemIdentifier) < (right,$1.calendarItemIdentifier)
                    }.prefix(limit+1).map(row)
                }
                output(["records":Array(records.prefix(limit)),"truncated":records.count > limit]); return
            }
            let resourceId = try string(q["resourceId"], max:1024)
            guard let calendar = selected.first(where:{$0.calendarIdentifier == resourceId}), calendar.allowsContentModifications else { throw Failure.readonly }
            if app == "calendar" {
                guard operation != "complete" else { throw Failure.invalid }
                let event: EKEvent
                if operation == "create" { event = EKEvent(eventStore:store); event.calendar = calendar }
                else {
                    let id = try string(q["id"],max:1024)
                    guard let existing = store.event(withIdentifier:id), existing.calendar.calendarIdentifier == resourceId,
                          let expected = q["expected"] as? [String:Any] else { throw Failure.missing }
                    try unchanged(row(existing),expected)
                    guard !existing.hasRecurrenceRules, !existing.isDetached, !existing.hasAttendees else { throw Failure.invalid }
                    event = existing
                }
                if operation == "trash" {
                    // EventKit has no recoverable trash. The server presents this explicitly before approval.
                    try store.remove(event,span:.thisEvent,commit:true)
                    guard store.event(withIdentifier:try string(q["id"],max:1024)) == nil else { throw Failure.uncertain }
                    output(["deleted":true]); return
                }
                if let title = q["title"] { event.title = try string(title,max:800) }
                if let body = q["body"] { event.notes = try string(body) }
                let start = q["start"] != nil ? try date(q["start"]) : event.startDate
                let end = q["end"] != nil ? try date(q["end"]) : canonicalEnd(event)
                let allDay = q["allDay"] as? Bool ?? event.isAllDay
                guard let title = event.title, !title.trimmingCharacters(in:.whitespacesAndNewlines).isEmpty,
                      let start, let end, end > start else { throw Failure.invalid }
                if allDay {
                    for boundary in [start,end] {
                        let c = Calendar.current.dateComponents([.hour,.minute,.second],from:boundary)
                        guard c.hour == 0, c.minute == 0, c.second == 0 else { throw Failure.invalid }
                    }
                }
                event.isAllDay = allDay
                event.startDate = start
                event.endDate = allDay ? end.addingTimeInterval(-1) : end
                try store.save(event,span:.thisEvent,commit:true)
                guard let saved = store.event(withIdentifier:event.eventIdentifier) else { throw Failure.uncertain }
                output(["record":try row(saved)]); return
            }
            let reminder: EKReminder
            if operation == "create" { reminder = EKReminder(eventStore:store); reminder.calendar = calendar }
            else {
                let id = try string(q["id"],max:1024)
                guard let existing = store.calendarItem(withIdentifier:id) as? EKReminder,
                      existing.calendar.calendarIdentifier == resourceId,
                      let expected = q["expected"] as? [String:Any] else { throw Failure.missing }
                try unchanged(row(existing),expected)
                guard !existing.hasRecurrenceRules else { throw Failure.invalid }
                reminder = existing
            }
            if operation == "trash" {
                let id = reminder.calendarItemIdentifier
                try store.remove(reminder,commit:true)
                guard store.calendarItem(withIdentifier:id) == nil else { throw Failure.uncertain }
                output(["deleted":true]); return
            }
            if let title = q["title"] { reminder.title = try string(title,max:800) }
            if let body = q["body"] { reminder.notes = try string(body) }
            if let due = q["due"] { reminder.dueDateComponents = try dueComponents(due) }
            if let priority = q["priority"] as? Int { guard [0,1,5,9].contains(priority) else { throw Failure.invalid }; reminder.priority = priority }
            if operation == "complete" {
                guard let value = q["completed"] as? Bool else { throw Failure.invalid }; reminder.isCompleted = value
            }
            guard !reminder.title.trimmingCharacters(in:.whitespacesAndNewlines).isEmpty else { throw Failure.invalid }
            try store.save(reminder,commit:true)
            guard let saved = store.calendarItem(withIdentifier:reminder.calendarItemIdentifier) as? EKReminder else { throw Failure.uncertain }
            output(["record":try row(saved)])
        } catch {
            output(["error":(error as? Failure)?.rawValue ?? "uncertain"]); exit(1)
        }
    }
}
