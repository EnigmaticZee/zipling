import Foundation

/// RAR samples. Nothing on a Mac can create RAR archives (the compressor is proprietary),
/// so these come from libarchive's test suite (github.com/libarchive/libarchive, v3.8.1,
/// `libarchive/test/`, BSD 2-clause licence). They're embedded rather than read from
/// disk so tests never touch the user's Documents folder.
enum Fixtures {
    /// `test_read_format_rar.rar`: RAR 4 with a file, a folder, an empty folder and a symbolic link.
    static let rar4Basic = Data(base64Encoded: [
        "UmFyIRoHAM+QcwAADQAAAAAAAACEUnQgkDIAFAAAABQAAAADQqLIvrd22j4UMAgApIEAAHRlc3QudHh0gAi3dto+t3baPnRl",
        "c3QgdGV4dCBkb2N1bWVudA0KnS90IJAyAAgAAAAIAAAAA3tEybbRTNg+FDAIAP+hAAB0ZXN0bGlua8AI0UzYPlBf2j50ZXN0",
        "LnR4dM3gdCCQOgAUAAAAFAAAAANCosi+Y3faPhQwEACkgQAAdGVzdGRpclx0ZXN0LnR4dMDMY3faPmN32j50ZXN0IHRleHQg",
        "ZG9jdW1lbnQNCqHIdOCQMQAAAAAAAAAAAAMAAAAAY3faPhQwBwDtQQAAdGVzdGRpcsDMY3faPmR32j7m53TgkDYAAAAAAAAA",
        "AAADAAAAAJ2r1T4UMAwA7UEAAHRlc3RlbXB0eWRpcoDMnavVPsVd2j7EPXsAQAcA",
    ].joined())!

    /// `test_read_format_rar5_symlink.rar`: RAR 5 with symbolic links to a file and to a folder.
    static let rar5Symlinks = Data(base64Encoded: [
        "UmFyIRoHAQAzkrXlCgEFBgAFAQGAgACEvYyuHgIChQAGhQCkgwI4jbdcIReTfYBAAQhmaWxlLnR4dDEyMzQKYxpTOS0CAw0A",
        "BgjtwwJXjbdcAAAAAIBAAQtzeW1saW5rLnR4dAwFAQAIZmlsZS50eHSOuvx1JAIDCAAGA+3DAkmWt1wAAAAAgEABB2Rpcmxp",
        "bmsHBQEBA2RpcqP/f8YXAgIABwDtgwFGlrdcAAAAAIAAAQNkaXIdd1ZRAwUEAA==",
    ].joined())!
}
