import Foundation
import MemorandumCore

extension CoreTests {
    func test_AC_MAC_001_ScreenshotUsesTopLeftDisplayCoordinates() throws {
        // 选框来自 NSView 本地坐标：原点左下；SCK 的 sourceRect 原点左上。
        let source = try XCTUnwrap(ScreenshotGeometry.sourceRect(
            localSelection: CGRect(x: 100, y: 600, width: 200, height: 100),
            screenSize: CGSize(width: 1440, height: 900), scale: 2))
        try XCTAssertEqual(source, CGRect(x: 100, y: 200, width: 200, height: 100))
        let lower = try XCTUnwrap(ScreenshotGeometry.sourceRect(
            localSelection: CGRect(x: 40, y: 50, width: 200, height: 120),
            screenSize: CGSize(width: 1440, height: 900), scale: 1))
        try XCTAssertEqual(lower, CGRect(x: 40, y: 730, width: 200, height: 120))
    }

    func test_AC_MAC_001_ScreenshotClampsAndAlignsToPixels() throws {
        let source = try XCTUnwrap(ScreenshotGeometry.sourceRect(
            localSelection: CGRect(x: -10, y: 870, width: 80, height: 80),
            screenSize: CGSize(width: 1440, height: 900), scale: 2))
        try XCTAssertEqual(source, CGRect(x: 0, y: 0, width: 70, height: 30))
        let fractional = try XCTUnwrap(ScreenshotGeometry.sourceRect(
            localSelection: CGRect(x: 10.2, y: 20.2, width: 30.4, height: 40.4),
            screenSize: CGSize(width: 1000, height: 800), scale: 2))
        try XCTAssertEqual(fractional, CGRect(x: 10, y: 739, width: 31, height: 41))
        try XCTAssertEqual(ScreenshotGeometry.sourceRect(localSelection: .zero,
            screenSize: CGSize(width: 1000, height: 800), scale: 2), nil)
    }

    func test_AC_MAC_001_ScreenshotDoesNotApplyGlobalScreenOffset() throws {
        // 副屏可在主屏左侧或上方；选区已经是屏幕内坐标，不应再减全局屏幕原点。
        let local = CGRect(x: 80, y: 100, width: 320, height: 160)
        for globalOrigin in [CGPoint.zero, CGPoint(x: -1920, y: 0), CGPoint(x: 0, y: 1080)] {
            let screenFrame = CGRect(origin: globalOrigin, size: CGSize(width: 1920, height: 1080))
            let source = try XCTUnwrap(ScreenshotGeometry.sourceRect(localSelection: local,
                screenSize: screenFrame.size, scale: 1))
            try XCTAssertEqual(source, CGRect(x: 80, y: 820, width: 320, height: 160))
        }
    }
}
