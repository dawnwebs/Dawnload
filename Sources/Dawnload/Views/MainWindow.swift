import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// The one window: intake on top, the list below, the inspector on the trailing edge.
struct MainWindow: View {
    @EnvironmentObject private var model: AppModel
    @State private var isDropTargeted = false
    @State private var inspectorSheet = false

    private static let inspectorWidth: CGFloat = 300
    private static let inspectorBreakpoint: CGFloat = 960

    var body: some View {
        GeometryReader { proxy in
            let wide = proxy.size.width >= Self.inspectorBreakpoint && !model.items.isEmpty
            HStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 0) {
                    IntakeView()
                        .padding(.horizontal, DLSpace.gutter)
                        .padding(.top, 10)
                        .padding(.bottom, 22)
                    ItemListView(onOpenInspector: { openInspector(wide: wide) })
                }
                .frame(maxWidth: .infinity)

                if model.showInspector && wide {
                    Rectangle().fill(DLColor.rule).frame(width: 1).ignoresSafeArea()
                    InspectorView()
                        .frame(width: Self.inspectorWidth)
                        .transition(.move(edge: .trailing))
                }
            }
            .animation(DLMotion.settle, value: model.showInspector && wide)
            .onChange(of: model.showInspector) { show in
                if show && !wide { inspectorSheet = true }
            }
        }
        .background(DLColor.canvas)
        .foregroundStyle(DLColor.text)
        .tint(DLColor.link)
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(DLColor.link, lineWidth: 2)
                    .padding(4)
                    .allowsHitTesting(false)
            }
        }
        .onDrop(of: [.fileURL, .url, .plainText], isTargeted: $isDropTargeted, perform: handleDrop)
        .toolbar { toolbar }
        .sheet(isPresented: $model.showFirstRun) {
            FirstRunView()
                .environmentObject(model)
        }
        .sheet(isPresented: $inspectorSheet) {
            VStack(spacing: 0) {
                InspectorView()
                    .environmentObject(model)
                HStack {
                    Spacer()
                    Button("Done") { inspectorSheet = false }
                        .keyboardShortcut(.defaultAction)
                }
                .padding(12)
            }
            .frame(width: 360, height: 560)
            .background(DLColor.canvas)
        }
        .background(WindowConfigurator())
        .navigationTitle("Dawnload")
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            HStack(spacing: 8) {
                DawnloadMark()
                    .frame(width: 15, height: 20)
                Wordmark()
                    .fill(DLColor.text)
                    .frame(width: 92, height: 14)
            }
            .padding(.leading, 4)
            .accessibilityElement()
            .accessibilityLabel("Dawnload")
        }
        ToolbarItem(placement: .automatic) {
            Spacer()
        }
        ToolbarItem(placement: .automatic) {
            ToolsPill()
        }
        ToolbarItem(placement: .automatic) {
            Button {
                model.showInspector.toggle()
            } label: {
                Image(systemName: "sidebar.right")
            }
            .help(model.showInspector ? "Hide Inspector (⌥⌘I)" : "Show Inspector (⌥⌘I)")
            .accessibilityLabel("Inspector")
        }
    }

    private func openInspector(wide: Bool) {
        if wide {
            model.showInspector = true
        } else {
            inspectorSheet = true
        }
    }

    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    guard let url else { return }
                    DispatchQueue.main.async { model.accept(dropped: url.isFileURL ? url.path : url.absoluteString) }
                }
                return true
            }
            if provider.canLoadObject(ofClass: URL.self) {
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    guard let url else { return }
                    DispatchQueue.main.async { model.accept(dropped: url.absoluteString) }
                }
                return true
            }
            if provider.canLoadObject(ofClass: String.self) {
                _ = provider.loadObject(ofClass: String.self) { text, _ in
                    guard let text else { return }
                    DispatchQueue.main.async { model.accept(dropped: text) }
                }
                return true
            }
        }
        return false
    }
}

/// "Tools ready", or the first tool that's missing. Opens Settings › Tools.
private struct ToolsPill: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Button {
            SettingsOpener.open(tab: .tools)
        } label: {
            HStack(spacing: 6) {
                Circle()
                    .fill(model.missingTools.isEmpty ? DLColor.kept : DLColor.failed)
                    .frame(width: 7, height: 7)
                Text(title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(model.missingTools.isEmpty ? DLColor.text2 : DLColor.text)
            }
            .padding(.horizontal, 6)
        }
        .help(model.tools.map { "\($0.tool.rawValue): \($0.path ?? "not found")" }.joined(separator: "\n"))
        .accessibilityLabel(title)
    }

    private var title: String {
        if let first = model.missingTools.first {
            return model.missingTools.count > 1 ? "\(first.rawValue) and \(model.missingTools.count - 1) more missing" : "\(first.rawValue) missing"
        }
        return "Tools ready"
    }
}

/// Paints the title bar in the canvas colour and drops its separator, so the window is one surface.
private struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { configure(view.window) }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        DispatchQueue.main.async { configure(nsView.window) }
    }

    private func configure(_ window: NSWindow?) {
        guard let window else { return }
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.titlebarSeparatorStyle = .none
        window.backgroundColor = DLColor.nsCanvas
        window.isMovableByWindowBackground = false
        window.toolbar?.allowsUserCustomization = false
    }
}
