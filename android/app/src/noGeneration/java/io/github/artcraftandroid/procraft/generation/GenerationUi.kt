package io.github.artcraftandroid.procraft.generation

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel

/** Image generation left out of the build (`procraft.generation=false`): nothing shows. */
object GenerationUi {
    const val enabled = false

    @Composable
    fun BarButton(vm: CanvasViewModel, width: Dp) = Unit

    @Composable
    fun SelectionButton(vm: CanvasViewModel, width: Dp) = Unit

    fun opened(session: io.github.artcraftandroid.procraft.core.CanvasSession, folder: String) = Unit

    fun openServer() = Unit

    fun leave(vm: CanvasViewModel) = Unit

    @Composable
    fun Panel(vm: CanvasViewModel, arrowX: Dp, modifier: Modifier) = Unit

    @Composable
    fun ResultsTray(vm: CanvasViewModel, modifier: Modifier) = Unit

    fun trayUp(): Boolean = false

    fun tuck() = Unit

    @Composable
    fun StyleStudio(vm: CanvasViewModel) = Unit
}
