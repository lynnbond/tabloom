// macOS-only native pasteboard integration. Original contents stay in memory.
import AppKit
import Foundation

func verify() throws {
let board = NSPasteboard.general
let original: [NSPasteboardItem] = (board.pasteboardItems ?? []).map { item in
    let copy = NSPasteboardItem()
    for type in item.types {
        if let data = item.data(forType: type) { copy.setData(data, forType: type) }
    }
    return copy
}
let fixture = "{\r\n\t\"中文😀\": \"<b>literal</b>\",\n\t\"ok\": true\r\n}\r"
let seeded = NSPasteboardItem()
seeded.setString(fixture, forType: .string)
seeded.setString("<b>rich alternative</b>", forType: .html)
seeded.setString("{\\rtf1 rich alternative}", forType: .rtf)
board.clearContents()
board.writeObjects([seeded])
defer {
    // Do not overwrite unrelated clipboard changes made by the user during this probe.
    if board.string(forType: .string) == fixture {
        board.clearContents()
        board.writeObjects(original)
    }
}
let process = Process()
process.executableURL = URL(fileURLWithPath: CommandLine.arguments[1])
process.arguments = [CommandLine.arguments[2]]
    try process.run()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else { throw NSError(domain: "Tabloom clipboard probe", code: 1) }
    guard board.string(forType: .string) == fixture else { throw NSError(domain: "Tabloom plain text mismatch", code: 2) }
    let types = board.types ?? []
    guard !types.contains(.html), !types.contains(.rtf), !types.contains(.png), !types.contains(.tiff) else {
        throw NSError(domain: "Tabloom rich content detected", code: 3)
    }
    print("MACOS_PASTEBOARD_OK: exact Unicode/CRLF text; no HTML/RTF/image; types=\(types.map { $0.rawValue })")
}
do { try verify() } catch {
    FileHandle.standardError.write(Data("Clipboard verification failed: \(error)\n".utf8))
    exit(1)
}
