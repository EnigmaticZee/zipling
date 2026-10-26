// Appearance sheet: each icon large, in macOS's Dark, Clear and Tinted styles, and at small sizes.
// Usage: swift modes.swift <modes-dir> <sheet.png> <tag>:<label> ...
// Expects <modes-dir>/<tag>-<Rendition>.png for Default, Dark, ClearLight, ClearDark, TintedLight, TintedDark.
import AppKit
import CoreGraphics
import CoreText

let args = CommandLine.arguments
let dir = URL(filePath: args[1])
let items = args.dropFirst(3).map { $0.split(separator: ":", maxSplits: 1).map(String.init) }
let cs = CGColorSpace(name: CGColorSpace.displayP3)!

func load(_ tag: String, _ r: String) -> CGImage? {
    guard let src = CGImageSourceCreateWithURL(dir.appending(path: "\(tag)-\(r).png") as CFURL, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(src, 0, nil)
}
func gray(_ v: CGFloat) -> CGColor { CGColor(colorSpace: cs, components: [v, v, v, 1])! }
func text(_ ctx: CGContext, _ s: String, _ p: CGPoint, _ size: CGFloat, _ w: NSFont.Weight, _ c: CGColor) {
    let a = NSAttributedString(string: s, attributes: [
        .font: NSFont.systemFont(ofSize: size, weight: w),
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): c,
    ])
    ctx.textPosition = p
    CTLineDraw(CTLineCreateWithAttributedString(a), ctx)
}
func panel(_ ctx: CGContext, _ r: CGRect, _ c: CGColor) {
    ctx.addPath(CGPath(roundedRect: r, cornerWidth: 18, cornerHeight: 18, transform: nil))
    ctx.setFillColor(c)
    ctx.fillPath()
}

let rowH = 360, W = 1900, H = 90 + items.count * rowH
let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.interpolationQuality = .high
ctx.setFillColor(gray(0.955))
ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))
text(ctx, "Zipling — every macOS appearance and size", CGPoint(x: 40, y: CGFloat(H - 60)), 30, .bold, gray(0.08))

let modes: [(String, String, CGColor)] = [("Dark", "Dark", gray(0.1)), ("ClearLight", "Clear light", rgb(0.78, 0.84, 0.95)),
                                          ("ClearDark", "Clear dark", gray(0.12)), ("TintedLight", "Tinted light", rgb(0.86, 0.88, 0.93)),
                                          ("TintedDark", "Tinted dark", gray(0.1))]
func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGColor { CGColor(colorSpace: cs, components: [r, g, b, 1])! }

for (i, item) in items.enumerated() {
    let y = CGFloat(H - 90 - (i + 1) * rowH)
    let tag = item[0]
    guard let big = load(tag, "Default") else { continue }
    ctx.draw(big, in: CGRect(x: 40, y: y + 40, width: 290, height: 290))
    text(ctx, item.count > 1 ? item[1] : tag, CGPoint(x: 40, y: y + 12), 20, .semibold, gray(0.1))
    for (k, (r, label, bg)) in modes.enumerated() {
        let box = CGRect(x: 370 + CGFloat(k) * 190, y: y + 100, width: 175, height: 200)
        panel(ctx, box, bg)
        if let img = load(tag, r) { ctx.draw(img, in: CGRect(x: box.minX + 22, y: box.minY + 42, width: 131, height: 131)) }
        text(ctx, label, CGPoint(x: box.minX + 16, y: box.minY + 14), 14, .medium, k == 1 || k == 3 ? gray(0.2) : gray(0.85))
    }
    // Small sizes on light and dark, inset to Apple's 824-in-1024 icon grid.
    for (k, bg) in [gray(0.9), gray(0.08)].enumerated() {
        let box = CGRect(x: 1340 + CGFloat(k) * 270, y: y + 100, width: 255, height: 200)
        panel(ctx, box, bg)
        var x = box.minX + 16
        for s in [96.0, 64.0, 32.0, 16.0] as [CGFloat] {
            let side = s * 824 / 1024
            ctx.draw(big, in: CGRect(x: x, y: box.midY - side / 2, width: side, height: side))
            x += side + 16
        }
    }
}
let data = NSBitmapImageRep(cgImage: ctx.makeImage()!).representation(using: .png, properties: [:])!
try! data.write(to: URL(filePath: args[2]))
print("wrote", args[2])
