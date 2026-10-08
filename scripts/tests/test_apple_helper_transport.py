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
    sys.platform == "darwin" and shutil.which("xcrun"),
    "unsupported: Apple EventKit SDK is available only on macOS",
)
class AppleHelperTransportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        try:
            sdk = subprocess.check_output(
                ["xcrun", "--sdk", "macosx", "--show-sdk-path"], text=True
            ).strip()
        except (OSError, subprocess.CalledProcessError) as error:
            raise unittest.SkipTest(f"unsupported: macOS SDK unavailable ({error})")
        if not (pathlib.Path(sdk) / "System/Library/Frameworks/EventKit.framework").exists():
            raise unittest.SkipTest("unsupported: macOS SDK does not contain EventKit")

        cls._temporary = tempfile.TemporaryDirectory(prefix="heiwa-apple-helper-tests-")
        cls.addClassCleanup(cls._temporary.cleanup)
        temporary = pathlib.Path(cls._temporary.name)
        source = HELPER_SOURCE.read_text()
        harness_source = cls._reader_harness_source(source)
        harness_path = temporary / "AppleHelperTransportHarness.swift"
        harness_path.write_text(harness_source)
        cls.harness = temporary / "apple-helper-transport-harness"
        cls.helper = temporary / "heiwa-apple-resources"
        target = f"{platform.machine()}-apple-macosx27.0"
        cls._compile(harness_path, cls.harness, target)
        cls._compile(HELPER_SOURCE, cls.helper, target)

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
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn(b"PRIVATE_SENTINEL", result.stdout + result.stderr)
                response = json.loads(result.stdout)
                self.assertEqual(response["schema_version"], 1)
                self.assertIn("error", response)


if __name__ == "__main__":
    unittest.main()
