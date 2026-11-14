import Foundation
import CryptoKit
import Darwin

enum HeiwaFiles {
    static func perform(_ request: [String: Any]) throws -> [String: Any] {
        guard let root = request["root"] as? String, root.hasPrefix("/"),
              let relative = request["path"] as? String, !relative.hasPrefix("/"), relative.utf8.count <= 4096,
              let operation = request["operation"] as? String, ["read", "create"].contains(operation)
        else { throw HeiwaApple.Failure.invalid }
        var contentToWrite: Data? = nil
        if operation == "create" {
            guard let content = request["content"] as? String, content.utf8.count <= 64000 else { throw HeiwaApple.Failure.invalid }
            contentToWrite = Data(content.utf8)
        }
        let pieces = relative.split(separator:"/", omittingEmptySubsequences:false).map(String.init)
        guard !pieces.isEmpty, pieces.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." && !$0.contains("\0") && $0.utf8.count <= 255 }) else { throw HeiwaApple.Failure.invalid }
        let canonical = URL(fileURLWithPath:root).resolvingSymlinksInPath().path
        var directory = open(canonical, O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
        guard directory >= 0 else { throw HeiwaApple.Failure.permission }
        defer { close(directory) }
        for piece in pieces.dropLast() {
            let next = openat(directory, piece, O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
            guard next >= 0 else { throw HeiwaApple.Failure.missing }
            close(directory); directory = next
        }
        let name = pieces.last!
        let flags = operation == "create" ? O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW : O_RDONLY | O_NOFOLLOW | O_NONBLOCK
        let fd = openat(directory, name, flags, mode_t(0o600))
        guard fd >= 0 else { throw HeiwaApple.Failure.missing }
        defer { close(fd) }
        if operation == "create" {
            let data = contentToWrite!
            var written = 0
            while written < data.count {
                let count = data.withUnsafeBytes { bytes in Darwin.write(fd, bytes.baseAddress!.advanced(by:written), data.count-written) }
                if count < 0 && errno == EINTR { continue }
                guard count > 0 else { throw HeiwaApple.Failure.uncertain }; written += count
            }
            guard fsync(fd) == 0, fsync(directory) == 0, lseek(fd, 0, SEEK_SET) == 0 else { throw HeiwaApple.Failure.uncertain }
        }
        var info = stat()
        guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG, info.st_size <= 64000 else { throw HeiwaApple.Failure.invalid }
        var bytes = [UInt8](repeating:0,count:64001), used = 0
        while used < bytes.count {
            let available = bytes.count - used
            let count = bytes.withUnsafeMutableBytes { Darwin.read(fd,$0.baseAddress!.advanced(by:used),available) }
            if count < 0 && errno == EINTR { continue }
            guard count >= 0 else { throw HeiwaApple.Failure.uncertain }
            if count == 0 { break }; used += count
        }
        guard used <= 64000, let content = String(bytes:bytes.prefix(used),encoding:.utf8) else { throw HeiwaApple.Failure.invalid }
        var named = stat()
        guard fstatat(directory, name, &named, AT_SYMLINK_NOFOLLOW) == 0, named.st_ino == info.st_ino, named.st_dev == info.st_dev else { throw HeiwaApple.Failure.uncertain }
        let digest = SHA256.hash(data:Data(bytes.prefix(used))).map { String(format:"%02x",$0) }.joined()
        if operation == "create", content != request["content"] as? String { throw HeiwaApple.Failure.uncertain }
        return ["path":relative,"content":content,"bytes":used,"sha256":digest,"created":operation == "create"]
    }
}
