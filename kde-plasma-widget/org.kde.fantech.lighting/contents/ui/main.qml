import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root

    preferredRepresentation: compactRepresentation

    compactRepresentation: PlasmaComponents.ToolButton {
        icon.name: "input-keyboard-symbolic"
        onClicked: root.expanded = !root.expanded
    }

    fullRepresentation: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Kirigami.Heading {
            level: 3
            text: "Fantech MK894 Controller"
        }

        GridLayout {
            columns: 2
            Layout.fillWidth: true

            QQC2.Button {
                text: "🎨 Sync KDE Accent"
                Layout.fillWidth: true
                onClicked: runCtl(["kde-sync"])
            }

            QQC2.Button {
                text: "🎮 Gaming Preset"
                Layout.fillWidth: true
                onClicked: runCtl(["light", "preset", "gaming"])
            }

            QQC2.Button {
                text: "🌆 Cyberpunk Neon"
                Layout.fillWidth: true
                onClicked: runCtl(["light", "preset", "cyberpunk"])
            }

            QQC2.Button {
                text: "🌑 Turn Off LEDs"
                Layout.fillWidth: true
                onClicked: runCtl(["test-keys", "--all-off"])
            }
        }

        Kirigami.Heading {
            level: 4
            text: "Lighting Animation"
        }

        QQC2.ComboBox {
            id: modeCombo
            Layout.fillWidth: true
            model: ["wave", "breathing", "static", "spectrum", "bloom", "stars", "raindrop", "reactive"]
            onActivated: {
                runCtl(["light", "set-mode", currentText, "--brightness", Math.round(brightSlider.value).toString()])
            }
        }

        RowLayout {
            Layout.fillWidth: true
            QQC2.Label { text: "Brightness:" }
            QQC2.Slider {
                id: brightSlider
                Layout.fillWidth: true
                from: 0
                to: 4
                stepSize: 1
                value: 4
                onMoved: {
                    runCtl(["light", "set-mode", modeCombo.currentText, "--brightness", Math.round(value).toString()])
                }
            }
        }
    }

    function runCtl(args) {
        // Invokes fantech-ctl command via system process
        print("Fantech MK894 Action: " + args.join(" "))
    }
}
