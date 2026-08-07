import Foundation
@preconcurrency import WebKit

enum SnapshotBuilder {
    @MainActor
    static var userScript: WKUserScript { BrowserUserScripts.snapshot }

    static func decode(_ value: Any?, generation: Int) throws -> BrowserSnapshot {
        guard let value, JSONSerialization.isValidJSONObject(value) else {
            throw BrowserError.jsError(hint: "Snapshot script did not return an object")
        }
        let data = try JSONSerialization.data(withJSONObject: value)
        let nodes = try JSONDecoder().decode([BrowserSnapshotNode].self, from: data)
        return BrowserSnapshot(generation: generation, nodes: nodes)
    }

    static func decodeConsole(_ value: Any?) throws -> [BrowserConsoleEntry] {
        guard let value, JSONSerialization.isValidJSONObject(value) else {
            throw BrowserError.jsError(hint: "Console script did not return an array")
        }
        let data = try JSONSerialization.data(withJSONObject: value)
        return try JSONDecoder().decode([BrowserConsoleEntry].self, from: data)
    }

    static func jsonString(from value: Any?) throws -> String {
        guard let value else { return "null" }
        if JSONSerialization.isValidJSONObject(value) {
            let data = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
            return String(decoding: data, as: UTF8.self)
        }
        if let string = value as? String {
            return String(decoding: try JSONEncoder().encode(string), as: UTF8.self)
        }
        if let number = value as? NSNumber {
            return number.stringValue
        }
        throw BrowserError.jsError(hint: "JavaScript returned an unsupported value")
    }

    static func actionScript(_ action: BrowserAct) throws -> String {
        let target = try targetExpression(action)
        switch action {
        case .click:
            return "(() => { const e = \(target); if (!e) throw new Error('target not found'); e.click(); })()"
        case .fill(_, _, let value):
            return "(() => { const e = \(target); if (!e) throw new Error('target not found'); e.focus(); e.value = \(try literal(value)); e.dispatchEvent(new Event('input', {bubbles:true})); e.dispatchEvent(new Event('change', {bubbles:true})); })()"
        case .type(_, _, let value):
            return "(() => { const e = \(target); if (!e) throw new Error('target not found'); e.focus(); for (const c of \(try literal(value))) { e.value += c; e.dispatchEvent(new InputEvent('input', {bubbles:true, data:c, inputType:'insertText'})); } })()"
        case .press(_, _, let key):
            return "(() => { const e = \(target) || document.activeElement; if (!e) throw new Error('target not found'); e.dispatchEvent(new KeyboardEvent('keydown', {key:\(try literal(key)), bubbles:true})); e.dispatchEvent(new KeyboardEvent('keyup', {key:\(try literal(key)), bubbles:true})); })()"
        case .scroll(_, _, let deltaX, let deltaY):
            return "window.scrollBy(\(deltaX), \(deltaY))"
        }
    }

    private static func targetExpression(_ action: BrowserAct) throws -> String {
        let ref: String?
        let selector: String?
        switch action {
        case .click(let r, let s), .fill(let r, let s, _), .type(let r, let s, _),
             .press(let r, let s, _), .scroll(let r, let s, _, _):
            ref = r; selector = s
        }
        if let ref {
            return "document.querySelector('[data-tiller-ref=" + (try literal(ref)) + "]')"
        }
        if let selector { return "document.querySelector(\(try literal(selector)))" }
        return "document.activeElement"
    }

    private static func literal(_ value: String) throws -> String {
        String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
    }
}
