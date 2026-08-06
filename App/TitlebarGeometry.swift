import CoreGraphics

enum TitlebarGeometry {
    static let accessoryHeight = AppTheme.titleStripHeight
    static let controlFrame = AppTheme.titlebarControlFrame
    static let controlSpacing = AppTheme.titlebarControlSpacing
    static let iconSize = AppTheme.titleStripIconSize
    static let trafficLightInset = AppTheme.trafficLightInset
    static let sidebarVerticalCorrection: CGFloat = 0

    static func verticalCenter(in accessoryHeight: CGFloat) -> CGFloat {
        accessoryHeight / 2
    }

    static func controlFrames(count: Int, in bounds: CGRect) -> [CGRect] {
        guard count > 0 else { return [] }

        let totalWidth = CGFloat(count) * controlFrame.width
            + CGFloat(count - 1) * controlSpacing
        let minimumX = bounds.minX
        let maximumX = bounds.maxX - totalWidth
        let centeredX = bounds.midX - totalWidth / 2
        let originX = min(max(centeredX, minimumX), maximumX)
        let minimumY = bounds.minY
        let maximumY = bounds.maxY - controlFrame.height
        let centeredY = bounds.midY - controlFrame.height / 2
        let originY = min(max(centeredY, minimumY), maximumY)

        return (0..<count).map { index in
            CGRect(
                x: originX + CGFloat(index) * (controlFrame.width + controlSpacing),
                y: originY,
                width: controlFrame.width,
                height: controlFrame.height
            )
        }
    }
}
