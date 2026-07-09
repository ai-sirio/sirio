#!/usr/bin/env swift
// Genera i layer dell'icona Tiller (minimal geometrico: raggi del timone a
// 8 punte, chiari su sfondo navy quasi-nero) come PNG 1024x1024.
// Uso: swift Tiller/Scripts/render-icon.swift <outputDir>
// Produce: background-1024.png (sfondo pieno), spokes-1024.png (raggi,
// trasparente), icon-1024.png (composita, base per l'appiconset).

import AppKit
import CoreGraphics

let size = 1024
let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."

func makeContext() -> CGContext {
    CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8,
        bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
}

func save(_ ctx: CGContext, _ name: String) {
    let url = URL(fileURLWithPath: outDir).appendingPathComponent(name)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    try! rep.representation(using: .png, properties: [:])!.write(to: url)
    print("wrote \(url.path)")
}

let navy = CGColor(red: 0.055, green: 0.078, blue: 0.125, alpha: 1)   // #0E1420
let light = CGColor(red: 0.91, green: 0.93, blue: 0.95, alpha: 1)     // #E8EDF2

func drawBackground(_ ctx: CGContext) {
    ctx.setFillColor(navy)
    ctx.fill(CGRect(x: 0, y: 0, width: size, height: size))
}

func drawSpokes(_ ctx: CGContext) {
    let c = CGFloat(size) / 2
    let spokeLength: CGFloat = 300   // dal centro verso l'esterno
    let spokeWidth: CGFloat = 56
    let innerGap: CGFloat = 96       // raggio del vuoto centrale
    let hubRadius: CGFloat = 60

    ctx.setFillColor(light)
    for i in 0..<8 {
        ctx.saveGState()
        ctx.translateBy(x: c, y: c)
        ctx.rotate(by: CGFloat(i) * .pi / 4)
        let spoke = CGRect(
            x: -spokeWidth / 2, y: innerGap,
            width: spokeWidth, height: spokeLength
        )
        let path = CGPath(
            roundedRect: spoke,
            cornerWidth: spokeWidth / 2, cornerHeight: spokeWidth / 2,
            transform: nil
        )
        ctx.addPath(path)
        ctx.fillPath()
        ctx.restoreGState()
    }
    // Mozzo centrale (anello)
    ctx.setStrokeColor(light)
    ctx.setLineWidth(44)
    ctx.strokeEllipse(in: CGRect(
        x: c - hubRadius, y: c - hubRadius,
        width: hubRadius * 2, height: hubRadius * 2
    ))
}

// Layer 1: solo sfondo
let bg = makeContext()
drawBackground(bg)
save(bg, "background-1024.png")

// Layer 2: solo raggi (trasparente)
let spokes = makeContext()
drawSpokes(spokes)
save(spokes, "spokes-1024.png")

// Composita: base per l'appiconset
let full = makeContext()
drawBackground(full)
drawSpokes(full)
save(full, "icon-1024.png")
