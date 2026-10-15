import Foundation

/// 输入必须是框选视图的本地坐标（左下原点、单位点）。
/// 输出为 ScreenCaptureKit 显示器内坐标（左上原点、单位点）。
public enum ScreenshotGeometry {
    public static func sourceRect(localSelection: CGRect, screenSize: CGSize, scale: CGFloat) -> CGRect? {
        guard scale.isFinite, scale > 0, screenSize.width.isFinite, screenSize.height.isFinite,
              screenSize.width > 0, screenSize.height > 0,
              [localSelection.minX, localSelection.minY, localSelection.width, localSelection.height].allSatisfy(\.isFinite) else { return nil }
        let bounds = CGRect(origin: .zero, size: screenSize)
        let selection = localSelection.standardized.intersection(bounds)
        guard !selection.isNull, selection.width > 0, selection.height > 0 else { return nil }
        let topLeft = CGRect(x: selection.minX, y: screenSize.height - selection.maxY,
                             width: selection.width, height: selection.height)
        // 在像素边界向外取整，sourceRect 与输出像素尺寸使用同一份几何数据。
        let x0 = floor(topLeft.minX * scale) / scale
        let y0 = floor(topLeft.minY * scale) / scale
        let x1 = ceil(topLeft.maxX * scale) / scale
        let y1 = ceil(topLeft.maxY * scale) / scale
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0).intersection(bounds)
    }
}
