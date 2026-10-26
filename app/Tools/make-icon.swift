// Renders the app icon (a stylised chest, for "Arca") into the asset catalog.
// Usage: swift app/Tools/make-icon.swift app/Arca/Resources/Assets.xcassets/AppIcon.appiconset
import AppKit
import CoreGraphics

let outDir = URL(filePath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ".")

func render(_ px: Int) -> Data {
    let s = CGFloat(px) / 1024
    let cs = CGColorSpace(name: CGColorSpace.displayP3)!
    let ctx = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
                        space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    // Work in a top-left, 1024-unit coordinate space.
    ctx.translateBy(x: 0, y: CGFloat(px))
    ctx.scaleBy(x: s, y: -s)

    func color(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
        CGColor(colorSpace: cs, components: [CGFloat((hex >> 16) & 0xFF) / 255, CGFloat((hex >> 8) & 0xFF) / 255, CGFloat(hex & 0xFF) / 255, a])!
    }
    func rounded(_ r: CGRect, _ radius: CGFloat) -> CGPath {
        CGPath(roundedRect: r, cornerWidth: radius, cornerHeight: radius, transform: nil)
    }

    // Body: the macOS icon grid's rounded square, with a soft drop shadow.
    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(0x000000, 0.28))
    ctx.addPath(rounded(body, 185))
    ctx.setFillColor(color(0x3B3FD1))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(rounded(body, 185))
    ctx.clip()
    let gradient = CGGradient(colorsSpace: cs, colors: [color(0x5B5FEF), color(0x2A9FD8), color(0x14C4C0)] as CFArray,
                              locations: [0, 0.62, 1])!
    ctx.drawLinearGradient(gradient, start: CGPoint(x: 160, y: 100), end: CGPoint(x: 860, y: 924), options: [])
    // A faint top highlight for depth.
    let shine = CGGradient(colorsSpace: cs, colors: [color(0xFFFFFF, 0.22), color(0xFFFFFF, 0)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(shine, start: CGPoint(x: 512, y: 100), end: CGPoint(x: 512, y: 520), options: [])
    ctx.restoreGState()

    // The chest: an arched lid over a box, split by a thin gap, with a clasp.
    let white = color(0xFFFFFF, 0.97)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: 10), blur: 24, color: color(0x101040, 0.30))
    ctx.beginTransparencyLayer(auxiliaryInfo: nil)

    let lid = CGMutablePath()
    lid.move(to: CGPoint(x: 262, y: 478))
    lid.addLine(to: CGPoint(x: 262, y: 400))
    lid.addCurve(to: CGPoint(x: 762, y: 400), control1: CGPoint(x: 290, y: 268), control2: CGPoint(x: 734, y: 268))
    lid.addLine(to: CGPoint(x: 762, y: 478))
    lid.closeSubpath()
    ctx.addPath(lid)
    ctx.setFillColor(white)
    ctx.fillPath()

    ctx.addPath(rounded(CGRect(x: 262, y: 506, width: 500, height: 236), 36))
    ctx.fillPath()

    // Bands across the lid and body.
    ctx.setBlendMode(.destinationOut)
    ctx.setFillColor(color(0x000000, 0.10))
    for x in [352.0, 640.0] {
        ctx.fill(CGRect(x: x, y: 300, width: 32, height: 442))
    }
    ctx.setBlendMode(.normal)

    // Clasp, bridging the gap, with a keyhole cut out.
    ctx.addPath(rounded(CGRect(x: 462, y: 440, width: 100, height: 118), 26))
    ctx.setFillColor(color(0xFFFFFF))
    ctx.fillPath()
    ctx.setBlendMode(.destinationOut)
    ctx.fillEllipse(in: CGRect(x: 494, y: 472, width: 36, height: 36))
    ctx.fill(CGRect(x: 505, y: 498, width: 14, height: 34))
    ctx.setBlendMode(.normal)

    ctx.endTransparencyLayer()
    ctx.restoreGState()

    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    return rep.representation(using: .png, properties: [:])!
}

let sizes: [(points: Int, scale: Int)] = [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]
var images: [[String: String]] = []
for (pt, scale) in sizes {
    let name = "icon_\(pt)x\(pt)\(scale == 2 ? "@2x" : "").png"
    try! render(pt * scale).write(to: outDir.appending(path: name))
    images.append(["idiom": "mac", "size": "\(pt)x\(pt)", "scale": "\(scale)x", "filename": name])
}
let contents: [String: Any] = ["images": images, "info": ["author": "xcode", "version": 1]]
try! JSONSerialization.data(withJSONObject: contents, options: [.prettyPrinted, .sortedKeys])
    .write(to: outDir.appending(path: "Contents.json"))
print("wrote \(images.count) icon images to \(outDir.path)")
