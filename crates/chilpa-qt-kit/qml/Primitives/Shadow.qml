import QtQuick.Effects
import app.chilpa.kit.primitives

// Own the native material before a model can remove a closed popup.
RectangularShadow {
    MaterialLifetime.ownMaterial: true
}
