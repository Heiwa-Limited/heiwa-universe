import Foundation
import EventKit

// Protocol data only. Rust owns enrollment, selection, persistence, and effects.
@main
struct AppleResources {
    static func main() async {
        do {
            let input = CommandLine.arguments.dropFirst().first ?? "{}"
            guard let request = try JSONSerialization.jsonObject(with: Data(input.utf8)) as? [String: Any],
                  let operation = request["operation"] as? String,
                  ["list", "scan"].contains(operation) else { throw ReadError.invalidRequest }
            let store = EKEventStore()
            if [.notDetermined, .writeOnly].contains(EKEventStore.authorizationStatus(for: .event)) {
                guard operation == "list", request["request_access"] as? Bool == true else { throw ReadError.permission }
                guard try await store.requestFullAccessToEvents() else { throw ReadError.permission }
            }
            guard EKEventStore.authorizationStatus(for: .event) == .fullAccess else { throw ReadError.permission }
            let calendars = store.calendars(for: .event)
            if operation == "list" {
                output(["schema_version": 1, "calendars": calendars.map { calendar in
                    ["id": calendar.calendarIdentifier, "name": calendar.title,
                     "source": calendar.source.title, "writable": calendar.allowsContentModifications] as [String: Any]
                }])
                return
            }
            let iso = ISO8601DateFormatter()
            guard let ids = request["calendar_ids"] as? [String], !ids.isEmpty, ids.count <= 100,
                  Set(ids).count == ids.count,
                  let startText = request["start"] as? String, let start = iso.date(from: startText),
                  let endText = request["end"] as? String, let end = iso.date(from: endText),
                  end > start, end.timeIntervalSince(start) <= 366 * 86400,
                  let limit = request["limit"] as? Int, (1...500).contains(limit) else { throw ReadError.invalidRequest }
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
            output(["schema_version": 1, "error": (error as? ReadError)?.rawValue ?? "calendar_read_failed"])
            exit(1)
        }
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
    }
}
