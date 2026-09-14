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
                  ["list", "scan", "plan_scan", "plan_apply"].contains(operation) else { throw ReadError.invalidRequest }
            let store = EKEventStore()
            if [.notDetermined, .writeOnly].contains(EKEventStore.authorizationStatus(for: .event)) {
                guard operation == "list", request["request_access"] as? Bool == true else { throw ReadError.permission }
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
    }
}
