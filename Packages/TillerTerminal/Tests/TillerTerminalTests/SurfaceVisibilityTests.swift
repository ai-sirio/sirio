import AppKit
import Testing
@testable import TillerTerminal

/// Fake surface view standing in for GhosttyTerminal.AppTerminalView:
/// records every visibility value applied to it.
private final class FakeSurfaceView: NSView, SurfaceOcclusionApplying {
    var applied: [Bool] = []
    func setSurfaceVisible(_ visible: Bool) { applied.append(visible) }
}

@MainActor
struct SurfaceVisibilityTests {

    @Test func applyReachesNestedSurfaceViews() {
        let root = NSView()
        let mid = NSView()
        let deep = FakeSurfaceView()
        let shallow = FakeSurfaceView()
        root.addSubview(shallow)
        root.addSubview(mid)
        mid.addSubview(deep)

        SurfaceVisibility.apply(false, in: root)

        #expect(shallow.applied == [false])
        #expect(deep.applied == [false])
    }

    @Test func containerAppliesVisibilityWhenFlagChanges() {
        let container = ContainerViewController()
        let child = NSViewController()
        child.view = NSView()
        let surface = FakeSurfaceView()
        child.view.addSubview(surface)
        container.setContent(child)

        container.surfacesVisible = false
        #expect(surface.applied.last == false)

        container.surfacesVisible = true
        #expect(surface.applied.last == true)
    }

    @Test func containerReappliesToLateMountedSurfacesOnLayout() {
        // NSHostingController materializes its AppKit subtree lazily; a
        // surface added after the flag was set must still pick it up on the
        // next layout pass.
        let container = ContainerViewController()
        let child = NSViewController()
        child.view = NSView()
        container.setContent(child)
        container.surfacesVisible = false

        let late = FakeSurfaceView()
        child.view.addSubview(late)
        #expect(late.applied.isEmpty)

        container.viewDidLayout()
        #expect(late.applied.last == false)
    }

    @Test func layoutAppliesCurrentVisibilityToAdoptedSurfaces() {
        // A pane adopted from a hidden host into a visible one arrives with
        // its surface still occluded; the visible host's layout pass must
        // un-occlude it even though its own flag never changed.
        let container = ContainerViewController()
        let child = NSViewController()
        child.view = NSView()
        let adopted = FakeSurfaceView()
        child.view.addSubview(adopted)
        container.setContent(child)

        container.viewDidLayout()
        #expect(adopted.applied.last == true)
    }
}
