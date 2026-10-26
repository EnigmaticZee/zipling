// Presentation sheet: candidates large, then in a Dock among real macOS icons (light and dark).
// Usage: swift present.swift <sheet.png> <cols> <icon.png>:<label> ...
import AppKit
import CoreGraphics
import CoreText

let args = CommandLine.arguments
let cols = Int(args[2])!
let items = args.dropFirst(3).map { $0.split(separator: ":", maxSplits: 1).map(String.init) }
let cs = CGColorSpace(name: CGColorSpace.displayP3)!

func load(_ path: String) -> CGImage? {
    guard let src = CGImageSourceCreateWithURL(URL(filePath: path) as CFURL, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(src, 0, nil)
}
func appIcon(_ path: String) -> CGImage? {
    var r = CGRect(x: 0, y: 0, width: 256, height: 256)
    return NSWorkspace.shared.icon(forFile: path).cgImage(forProposedRect: &r, context: nil, hints: nil)
}
func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGColor { CGColor(colorSpace: cs, components: [r, g, b, 1])! }
func text(_ ctx: CGContext, _ s: String, _ p: CGPoint, _ size: CGFloat, _ w: NSFont.Weight, _ c: CGColor) {
    let a = NSAttributedString(string: s, attributes: [
        .font: NSFont.systemFont(ofSize: size, weight: w),
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): c,
    ])
    ctx.textPosition = p
    CTLineDraw(CTLineCreateWithAttributedString(a), ctx)
}

let system = ["/System/Applications/Preview.app", "/System/Applications/Notes.app",
              "/System/Library/CoreServices/Applications/Archive Utility.app", "/System/Applications/Chess.app",
              "/Applications/Burnish.app", "/System/Applications/Utilities/Terminal.app"].compactMap(appIcon)
let icons = items.map { load($0[0])! }

let cell = 400, rows = (items.count + cols - 1) / cols
let dockIcon: CGFloat = 92, gap: CGFloat = 14
// Dock order: system icons interleaved with the candidates.
// ictool exports fill the canvas; system icons sit on Apple's 824-in-1024 grid, so inset ours to match.
var dockRow: [(CGImage, CGFloat)] = []
for (i, ic) in icons.enumerated() {
    dockRow.append((ic, 824.0 / 1024.0))
    if i < system.count { dockRow.append((system[i], 1.0)) }
}
let dockW = CGFloat(dockRow.count) * (dockIcon + gap) + gap
let W = max(60 + cols * cell, Int(dockW) + 120)
let dockBlock = 2 * 200 + 60
let H = 110 + rows * cell + dockBlock
let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.interpolationQuality = .high
ctx.setFillColor(rgb(0.955, 0.955, 0.965))
ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))
text(ctx, "Zipling — finalists in the Dock", CGPoint(x: 40, y: H - 66), 32, .bold, rgb(0.08, 0.08, 0.1))

for (i, item) in items.enumerated() {
    let x = CGFloat(40 + (i % cols) * cell)
    let y = CGFloat(H - 110 - (i / cols + 1) * cell)
    ctx.draw(icons[i], in: CGRect(x: x + 20, y: y + 50, width: 330, height: 330))
    text(ctx, item.count > 1 ? item[1] : "", CGPoint(x: x + 34, y: y + 18), 20, .semibold, rgb(0.1, 0.1, 0.12))
}

// Two Docks: over a light and a dark wallpaper.
for (k, (top, bottom)) in [(rgb(0.80, 0.86, 0.97), rgb(0.95, 0.86, 0.80)), (rgb(0.10, 0.12, 0.22), rgb(0.03, 0.04, 0.08))].enumerated() {
    let band = CGRect(x: 40, y: CGFloat(30 + (1 - k) * 210), width: CGFloat(W - 80), height: 190)
    ctx.saveGState()
    ctx.addPath(CGPath(roundedRect: band, cornerWidth: 26, cornerHeight: 26, transform: nil))
    ctx.clip()
    let grad = CGGradient(colorsSpace: cs, colors: [top, bottom] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(grad, start: CGPoint(x: band.minX, y: band.maxY), end: CGPoint(x: band.maxX, y: band.minY), options: [])
    let dock = CGRect(x: band.midX - dockW / 2, y: band.minY + 30, width: dockW, height: dockIcon + 2 * gap)
    ctx.addPath(CGPath(roundedRect: dock, cornerWidth: 30, cornerHeight: 30, transform: nil))
    ctx.setFillColor(k == 0 ? CGColor(colorSpace: cs, components: [1, 1, 1, 0.45])! : CGColor(colorSpace: cs, components: [0.2, 0.2, 0.25, 0.55])!)
    ctx.fillPath()
    for (j, (ic, scale)) in dockRow.enumerated() {
        let side = dockIcon * scale, pad = (dockIcon - side) / 2
        ctx.draw(ic, in: CGRect(x: dock.minX + gap + CGFloat(j) * (dockIcon + gap) + pad, y: dock.minY + gap + pad, width: side, height: side))
    }
    ctx.restoreGState()
}
let data = NSBitmapImageRep(cgImage: ctx.makeImage()!).representation(using: .png, properties: [:])!
try! data.write(to: URL(filePath: args[1]))
print("wrote", args[1])
