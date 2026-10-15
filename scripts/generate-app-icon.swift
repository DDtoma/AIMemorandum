import AppKit
import Foundation

guard CommandLine.arguments.count == 2 else { fatalError("Usage: generate-app-icon.swift <iconset-directory>") }
let target = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8,
            bytesPerRow: pixels * 4, space: colorSpace, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.scaleBy(x: CGFloat(pixels) / 1024, y: CGFloat(pixels) / 1024)
        func fill(_ rect: CGRect, radius: CGFloat, color: CGColor) {
            context.setFillColor(color)
            context.addPath(CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil))
            context.fillPath()
        }
        fill(CGRect(x: 64, y: 64, width: 896, height: 896), radius: 196,
             color: CGColor(srgbRed: 0.18, green: 0.36, blue: 0.28, alpha: 1))
        fill(CGRect(x: 270, y: 224, width: 484, height: 600), radius: 46,
             color: CGColor(srgbRed: 0.98, green: 0.98, blue: 0.94, alpha: 1))
        for (y, width) in [(CGFloat(676), CGFloat(294)), (568, 250), (460, 204)] {
            fill(CGRect(x: 350, y: y, width: width, height: 28), radius: 14,
                 color: CGColor(srgbRed: 0.38, green: 0.52, blue: 0.44, alpha: 1))
        }
        fill(CGRect(x: 556, y: 150, width: 282, height: 282), radius: 141,
             color: CGColor(srgbRed: 0.68, green: 0.84, blue: 0.71, alpha: 1))
        context.setStrokeColor(CGColor(srgbRed: 0.14, green: 0.31, blue: 0.22, alpha: 1))
        context.setLineWidth(34)
        context.setLineCap(.round)
        context.setLineJoin(.round)
        context.move(to: CGPoint(x: 624, y: 288))
        context.addLine(to: CGPoint(x: 679, y: 234))
        context.addLine(to: CGPoint(x: 772, y: 344))
        context.strokePath()
        let rep = NSBitmapImageRep(cgImage: context.makeImage()!)
        let name = "icon_\(size)x\(size)" + (scale == 2 ? "@2x" : "") + ".png"
        try rep.representation(using: .png, properties: [:])!.write(to: target.appendingPathComponent(name))
    }
}
