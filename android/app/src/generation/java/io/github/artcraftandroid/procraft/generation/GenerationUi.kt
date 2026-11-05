package io.github.artcraftandroid.procraft.generation

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.Panel
import io.github.artcraftandroid.procraft.core.Generation
import io.github.artcraftandroid.procraft.core.GenerationException
import io.github.artcraftandroid.procraft.core.GenerationSettings
import io.github.artcraftandroid.procraft.core.GenerationState
import io.github.artcraftandroid.procraft.core.RunSettings
import io.github.artcraftandroid.procraft.core.CanvasSession
import io.github.artcraftandroid.procraft.core.GenerationResolution
import io.github.artcraftandroid.procraft.core.GenerationMode
import io.github.artcraftandroid.procraft.core.SelectionInfo
import io.github.artcraftandroid.procraft.ui.ToolbarButton
import io.github.artcraftandroid.procraft.core.SizeInfo
import io.github.artcraftandroid.procraft.core.ServerConnection
import io.github.artcraftandroid.procraft.core.ServerStatus
import io.github.artcraftandroid.procraft.core.ThumbnailImage
import io.github.artcraftandroid.procraft.core.generationAddress
import io.github.artcraftandroid.procraft.ui.ActionButton
import io.github.artcraftandroid.procraft.ui.Pc
import io.github.artcraftandroid.procraft.ui.Popover
import io.github.artcraftandroid.procraft.ui.SliderRow
import kotlinx.coroutines.delay
import java.nio.ByteBuffer

/**
 * Image generation, as the desktop's (generation.rs): the Generate button on the top bar (ringed
 * with a run's progress), its popover (the styles, the prompt, strength, seed and count, and
 * Generate; and its server view: the address, what the server has, NSFW), and the results bar at
 * the foot of the screen while a run goes or there are results. What runs is the core's
 * (generation.rs); this shows it and polls it. Built only with `procraft.generation`.
 */
object GenerationUi {
    const val enabled = true

    private val OK = Color(0xFF3CCF6E)
    private val WARN = Color(0xFFF2B440)
    private val BAD = Color(0xFFE84F51)
    private val Field get() = if (Pc.light) Color(0x12000000) else Color(0x12FFFFFF)
    private val RowFill get() = if (Pc.light) Color(0xFFD8D8D8) else Color(0x0DFFFFFF)
    private val Card get() = if (Pc.light) Color(0xFFD7D7D7) else Color(0x2E5B5B5B)
    private val Dim get() = Pc.TextDim
    private val Ink get() = Pc.Text

    /** "AI", drawn from the style guide's capsules: A's legs and the I 3 units thick, its bar 2.4. */
    val Icon: ImageVector by lazy {
        fun ImageVector.Builder.capsules(width: Float, draw: androidx.compose.ui.graphics.vector.PathBuilder.() -> Unit) =
            path(fill = null, stroke = SolidColor(Color.White), strokeLineWidth = width, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round, pathBuilder = draw)
        ImageVector.Builder("generate", 44.dp, 44.dp, 44f, 44f).apply {
            capsules(3f) { moveTo(14.2f, 30f); lineTo(19.2f, 14f); lineTo(24.2f, 30f) }
            capsules(2.4f) { moveTo(16.4f, 24.4f); lineTo(22f, 24.4f) }
            capsules(3f) { moveTo(29.6f, 14f); lineTo(29.6f, 30f) }
        }.build()
    }

    private enum class View { FORM, SERVER }

    // the app's Generation, made with the settings kept, and the form's and bar's state
    private var generation: Generation? = null
    private var settings by mutableStateOf(GenerationSettings("", false))
    private var view by mutableStateOf(View.FORM)
    // the prompt with its caret (the tag suggestions need where it is)
    private var promptField by mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(""))
    private var prompt: String
        get() = promptField.text
        set(value) { promptField = androidx.compose.ui.text.input.TextFieldValue(value, androidx.compose.ui.text.TextRange(value.length)) }
    private var promptFocused by mutableStateOf(false)
    private var style by mutableStateOf("")
    private var strength by mutableStateOf(1f)
    private var seed by mutableStateOf<ULong?>(null)
    private var count by mutableStateOf(1)
    private var resolution by mutableStateOf(GenerationResolution.QUALITY)
    // the selection's mode as picked (none: the guess), and the selection it was picked for
    private var mode by mutableStateOf<GenerationMode?>(null)
    private var modeFor: SelectionInfo? = null
    // each control layer's strength while the app runs, by layer id (else its kind's default)
    private val controlStrengths = mutableStateMapOf<ULong, Float>()
    // the result held down, showing its Delete
    private var deleting by mutableStateOf<ULong?>(null)
    private var address by mutableStateOf("")
    private var state by mutableStateOf(GenerationState(null, emptyList(), null, null))
    private var server by mutableStateOf<ServerStatus?>(null)
    private var last: RunSettings? = null
    private var error by mutableStateOf<String?>(null)
    private val bitmaps = mutableStateMapOf<ULong, ImageBitmap>()
    private var preview by mutableStateOf<Pair<Long, ImageBitmap>?>(null)

    private fun prefs(context: Context) = context.getSharedPreferences("generation", Context.MODE_PRIVATE)

    // the app's context, for keeping the recent prompts after a run
    private var appContext: Context? = null

    private fun generation(context: Context): Generation = generation ?: run {
        appContext = context.applicationContext
        val p = prefs(context)
        val largest = p.getFloat("largest", 0f).takeIf { it > 0f }
        settings = GenerationSettings(p.getString("address", "") ?: "", p.getBoolean("nsfw", false), largest)
        resolution = GenerationResolution.entries.firstOrNull { it.name == p.getString("resolution", null) } ?: GenerationResolution.QUALITY
        address = settings.address
        Generation(settings).also { g ->
            generation = g
            g.setRecentPrompts(p.getString("recent", "")?.split('\n')?.filter { it.isNotBlank() } ?: emptyList())
            opened?.let { (s, f) -> g.setFolder(s, f) }
        }
    }

    private fun keep(context: Context) {
        prefs(context).edit().putString("address", settings.address).putBoolean("nsfw", settings.nsfw)
            .putFloat("largest", settings.largestMegapixels ?: 0f).putString("resolution", resolution.name).apply()
        generation(context).setSettings(settings)
    }

    /** Looks at the run and the server; makes bitmaps for new results. */
    private fun poll(g: Generation) {
        state = g.state()
        server = g.server()
        val ids = state.results.map { it.id }.toSet()
        bitmaps.keys.retainAll(ids)
        for (r in state.results) if (r.id !in bitmaps) bitmaps[r.id] = r.thumbnail.bitmap()
        val run = state.running
        val p = run?.preview
        if (run != null && p != null) {
            val key = (run.id.toLong() shl 32) or run.previews.toLong()
            if (preview?.first != key) preview = key to p.bitmap()
        }
    }

    private fun ThumbnailImage.bitmap(): ImageBitmap {
        val b = Bitmap.createBitmap(width.toInt(), height.toInt(), Bitmap.Config.ARGB_8888)
        b.copyPixelsFromBuffer(ByteBuffer.wrap(rgba))
        return b.asImageBitmap()
    }

    /** The top bar's Generate button: lit while its popover is open, ringed while a run goes. */
    @Composable
    fun BarButton(vm: CanvasViewModel) {
        val g = generation(LocalContext.current)
        // the run and the server looked at often while something is under way, else now and then
        LaunchedEffect(g) {
            while (true) {
                poll(g)
                val busy = state.running != null || server?.connection == ServerConnection.CONNECTING
                delay(if (busy || vm.panel == Panel.GENERATE) 100 else 1000)
            }
        }
        Box(contentAlignment = Alignment.Center) {
            ActionButton(Icon, "Generate", selected = vm.panel == Panel.GENERATE, glyph = 40.dp) { vm.toggle(Panel.GENERATE) }
            state.running?.let { ProgressRing(it.progress, Modifier.size(Pc.ActionDiscSize + 5.dp)) }
        }
    }

    /** The Selection toolbar's Generate button: closes the tool and opens the popover. */
    @Composable
    fun SelectionButton(vm: CanvasViewModel, width: Dp) {
        ToolbarButton(Icon, "Generate", enabled = vm.hasSelection, width = width, iconSize = 40.dp) {
            vm.closeSelecting()
            if (vm.panel != Panel.GENERATE) vm.toggle(Panel.GENERATE)
        }
    }

    /** The artwork just opened and its folder, where its results are kept. */
    private var opened: Pair<CanvasSession, String>? = null

    /** An artwork opened (or was made) in `folder`: its results, kept there, come back. */
    fun opened(session: CanvasSession, folder: String) {
        opened = session to folder
        generation?.setFolder(session, folder)
    }

    /** Opens the server view (from the Prefs tab). */
    fun openServer() {
        view = View.SERVER
    }

    /** The artwork is closing: its run is cancelled and its results dropped. */
    fun leave(vm: CanvasViewModel) {
        val s = vm.session ?: return
        generation?.leave(s)
        state = GenerationState(null, emptyList(), null, null)
    }

    /** The popover hanging from the Generate button. */
    @Composable
    fun Panel(vm: CanvasViewModel, arrowX: Dp, modifier: Modifier) {
        val context = LocalContext.current
        val g = generation(context)
        // above the keyboard while it's up: the form scrolls, Generate stays in view
        val ime = WindowInsets.ime.asPaddingValues().calculateBottomPadding()
        val screen = LocalConfiguration.current.screenHeightDp.dp
        // above the results bar too while it shows (96 tall, 22 above the screen's foot), not under it
        val room = when {
            ime > 0.dp -> Modifier.heightIn(max = screen - Pc.BarHeight - ime - 12.dp)
            hasResults() -> Modifier.heightIn(max = screen - Pc.BarHeight - 96.dp - 22.dp - 34.dp)
            else -> Modifier
        }
        Popover(400.dp, arrowX, modifier.then(room)) {
            when {
                view == View.SERVER -> ServerView(context, g)
                server?.connection == ServerConnection.NOT_SET -> NotSetUp()
                else -> Form(vm, g)
            }
        }
    }

    @Composable
    private fun Title() {
        Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 24.dp, end = 24.dp, top = 18.dp), verticalAlignment = Alignment.Top) {
            Text("Generate", color = Pc.PanelTitle, fontSize = 24.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.weight(1f))
            val s = server
            val (colour, words) = when (s?.connection) {
                ServerConnection.CONNECTED -> (if (s.features.all { it.ready }) OK else WARN) to s.address
                ServerConnection.CONNECTING -> WARN to "connecting"
                ServerConnection.FAILED -> BAD to s.address
                else -> BAD to "not set up"
            }
            // the server, as a button: its settings (address, NSFW, largest picture) behind it
            Row(
                Modifier.clip(RoundedCornerShape(10.dp)).background(RowFill).tap { view = View.SERVER }.padding(start = 10.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.size(9.dp).clip(CircleShape).background(colour))
                Spacer(Modifier.width(7.dp))
                Text(words, color = Ink, fontSize = 13.sp, maxLines = 1)
                Text("  ›", color = Dim, fontSize = 17.sp, maxLines = 1)
            }
        }
    }

    @Composable
    private fun Label(words: String) {
        Text(words, color = Dim, fontSize = 12.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 18.dp, top = 12.dp, bottom = 6.dp))
    }

    @Composable
    private fun ColumnScope.Form(vm: CanvasViewModel, g: Generation) {
        val focus = LocalFocusManager.current
        Title()
        val styles = remember(server) { g.styles() }
        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState())) {
            // the styles: cards in a row, the chosen one ringed
            Label("STYLE")
            if (styles.none { it.id == style }) styles.firstOrNull()?.let { style = it.id }
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (s in styles) {
                    val shape = RoundedCornerShape(12.dp)
                    Column(
                        Modifier.width(116.dp).height(76.dp).clip(shape).background(Card)
                            .then(if (s.id == style) Modifier.border(2.dp, Pc.Accent, shape) else Modifier)
                            .tap { style = s.id }.padding(10.dp),
                    ) {
                        Text(s.name, color = Ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 2, modifier = Modifier.weight(1f))
                        Text("${s.steps} steps · CFG ${s.cfg}", color = Dim, fontSize = 11.sp, maxLines = 1)
                    }
                }
                if (styles.isEmpty()) Text("No Anima model on the server", color = Dim, fontSize = 14.sp, modifier = Modifier.padding(vertical = 26.dp))
            }
            // the prompt
            Label("PROMPT")
            Box(Modifier.padding(horizontal = 16.dp).fillMaxWidth().heightIn(min = 82.dp).clip(RoundedCornerShape(12.dp)).background(Field).padding(horizontal = 12.dp, vertical = 10.dp)) {
                if (prompt.isEmpty()) Text("Describe it, or type tags…", color = Pc.Disabled, fontSize = 15.sp)
                BasicTextField(
                    promptField,
                    onValueChange = { promptField = it },
                    modifier = Modifier.fillMaxWidth().onFocusChanged { promptFocused = it.isFocused },
                    textStyle = TextStyle(color = Ink, fontSize = 15.sp),
                    cursorBrush = SolidColor(Pc.Accent),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                    maxLines = 6,
                )
            }
            // suggestions for the tag being typed, or (field empty) the recent prompts
            if (promptFocused) TagStrip(g)
            Spacer(Modifier.height(8.dp))
            // strength: the last stretch snaps to a new image
            Box(Modifier.padding(horizontal = 6.dp)) {
                val shown = if (strength >= 1f) "New image" else "${(strength * 100).toInt()}% · Refine"
                SliderRow("Strength", strength, shown) { t -> strength = if (t > 0.97f) 1f else Math.round(t * 100) / 100f }
            }
            // seed and count
            Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f).height(40.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Seed", color = Ink, fontSize = 14.sp, modifier = Modifier.weight(1f))
                    val s = seed
                    if (s == null) {
                        Text("Random", color = Dim, fontSize = 14.sp, modifier = Modifier.tap { seed = state.results.lastOrNull()?.seed ?: 1uL })
                    } else {
                        BasicTextField(
                            s.toString(),
                            onValueChange = { t -> seed = t.filter(Char::isDigit).take(18).toULongOrNull() ?: 0uL },
                            modifier = Modifier.width(110.dp),
                            singleLine = true,
                            textStyle = TextStyle(color = Ink, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.End),
                            cursorBrush = SolidColor(Pc.Accent),
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
                            keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                        )
                        Text("  ×", color = Dim, fontSize = 18.sp, modifier = Modifier.tap { seed = null })
                    }
                }
                Spacer(Modifier.width(8.dp))
                Row(Modifier.height(40.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).padding(3.dp)) {
                    for (n in 1..4) {
                        val on = count == n
                        Box(Modifier.width(36.dp).height(34.dp).clip(RoundedCornerShape(9.dp)).background(if (on) Pc.Accent else Color.Transparent).tap { count = n }, contentAlignment = Alignment.Center) {
                            Text("$n", color = if (on) Color.White else Dim, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                }
            }
            // control layers: one row each, its strength, and × to make it a plain layer again
            val controls = vm.session?.let { s -> g.controlLayers(s).filter { it.visible } } ?: emptyList()
            if (controls.isNotEmpty()) {
                Label("CONTROL LAYERS")
                for (c in controls) {
                    val default = if (c.kind == io.github.artcraftandroid.procraft.core.LayerControl.LINE_ART) 0.7f else 0.5f
                    val value = controlStrengths[c.id] ?: default
                    Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Image(c.thumbnail.bitmap(), null, contentScale = ContentScale.Crop, modifier = Modifier.size(36.dp).clip(RoundedCornerShape(7.dp)).background(Color.White))
                        Spacer(Modifier.width(10.dp))
                        Column(Modifier.weight(1f)) {
                            Text(c.name, color = Ink, fontSize = 14.sp, maxLines = 1)
                            Text(if (c.kind == io.github.artcraftandroid.procraft.core.LayerControl.LINE_ART) "Line art" else "Scribble", color = Dim, fontSize = 12.sp)
                        }
                        Text("×", color = Dim, fontSize = 22.sp, modifier = Modifier.tap { vm.session?.setLayerControl(c.id, null); vm.refresh() }.padding(horizontal = 8.dp))
                    }
                    Box(Modifier.padding(horizontal = 6.dp)) {
                        SliderRow("Strength", value, "${(value * 100).toInt()}%") { t -> controlStrengths[c.id] = Math.round(t * 100) / 100f }
                    }
                }
            }
            // with a selection: how to fill it, the guess picked until another is
            val selection = vm.session?.let { g.selection(it) }
            if (selection != null && (modeFor?.x != selection.x || modeFor?.y != selection.y || modeFor?.width != selection.width || modeFor?.height != selection.height)) {
                mode = null
                modeFor = selection
            }
            if (selection != null) {
                Label("SELECTION")
                val chosen = mode ?: selection.mode
                Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth().height(40.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).padding(3.dp)) {
                    for ((choice, words) in listOf(GenerationMode.FILL to "Fill", GenerationMode.ADD to "Add", GenerationMode.REMOVE to "Remove", GenerationMode.EXPAND to "Expand")) {
                        val on = chosen == choice
                        Box(
                            Modifier.weight(1f).height(34.dp).clip(RoundedCornerShape(9.dp)).background(if (on) Pc.Accent else Color.Transparent).tap { mode = choice },
                            contentAlignment = Alignment.Center,
                        ) { Text(words, color = if (on) Color.White else Dim, fontSize = 14.sp, fontWeight = FontWeight.Medium) }
                    }
                }
                Spacer(Modifier.height(4.dp))
            }
            // resolution: Draft, Normal, Quality, and what that makes in small text
            val context = LocalContext.current
            Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth().height(40.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).padding(3.dp)) {
                for ((choice, words) in listOf(GenerationResolution.DRAFT to "Draft", GenerationResolution.NORMAL to "Normal", GenerationResolution.QUALITY to "Quality")) {
                    val on = resolution == choice
                    Box(
                        Modifier.weight(1f).height(34.dp).clip(RoundedCornerShape(9.dp)).background(if (on) Pc.Accent else Color.Transparent)
                            .tap { resolution = choice; keep(context) },
                        contentAlignment = Alignment.Center,
                    ) { Text(words, color = if (on) Color.White else Dim, fontSize = 14.sp, fontWeight = FontWeight.Medium) }
                }
            }
            vm.session?.let { session ->
                val size = g.sizeFor(session, resolution)
                Text(planText(size), color = Dim, fontSize = 12.sp, modifier = Modifier.padding(start = 20.dp, end = 20.dp, bottom = 4.dp))
            }
        }
        Column(Modifier.padding(bottom = 16.dp)) {
            // Generate, or Cancel while a run goes
            val running = state.running
            val selection = vm.session?.let { g.selection(it) }
            val picked = selection?.let { mode ?: it.mode }
            val needsPrompt = picked == GenerationMode.ADD && prompt.isBlank()
            val ready = server?.connection == ServerConnection.CONNECTED && styles.isNotEmpty() && !needsPrompt
            val words = when {
                running != null -> "Cancel · ${(running.progress * 100).toInt()}%"
                count > 1 -> "Generate $count"
                else -> "Generate"
            }
            val fill = if (running != null) Color(0xFF5A5A5A) else Pc.Accent
            Box(
                Modifier.padding(start = 16.dp, end = 16.dp, top = 6.dp).fillMaxWidth().height(48.dp).clip(RoundedCornerShape(14.dp))
                    .background(if (ready || running != null) fill else fill.copy(alpha = 0.4f))
                    .tap {
                        focus.clearFocus()
                        if (running != null) g.cancel() else if (ready) run(vm, g, RunSettings(prompt, style, strength, seed, count.toUInt(), resolution, picked, controlStrengths.map { (id, v) -> io.github.artcraftandroid.procraft.core.ControlSetting(id, v) }))
                    },
                contentAlignment = Alignment.Center,
            ) { Text(words, color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold) }
            if (needsPrompt && running == null) Text("Add needs a prompt: say what to put there.", color = Dim, fontSize = 13.sp, modifier = Modifier.padding(start = 18.dp, end = 18.dp, top = 10.dp))
            (error ?: state.error)?.let { Text(it, color = WARN, fontSize = 13.sp, modifier = Modifier.padding(start = 18.dp, end = 18.dp, top = 10.dp)) }
        }
    }

    private fun run(vm: CanvasViewModel, g: Generation, run: RunSettings) {
        val s = vm.session ?: return
        error = try {
            g.generate(s, run)
            last = run
            appContext?.let { c -> prefs(c).edit().putString("recent", g.recentPrompts().joinToString("\n") { it.replace('\n', ' ') }).apply() }
            null
        } catch (e: GenerationException) {
            e.message
        }
        poll(g)
    }

    @Composable
    private fun NotSetUp() {
        Title()
        Column(Modifier.fillMaxWidth().padding(bottom = 20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(Icon, null, tint = Dim, modifier = Modifier.size(54.dp))
            Spacer(Modifier.height(8.dp))
            Text("Generate needs ComfyUI running on a PC.", color = Ink, fontSize = 14.sp)
            Text("Set its address to connect.", color = Ink, fontSize = 14.sp)
            Spacer(Modifier.height(16.dp))
            Box(Modifier.width(200.dp).height(42.dp).clip(RoundedCornerShape(14.dp)).background(Pc.Accent).tap { view = View.SERVER }, contentAlignment = Alignment.Center) {
                Text("Set up", color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }

    /** The server: its address, what it has and what that's enough for, NSFW, and the notes. */
    @Composable
    private fun ServerView(context: Context, g: Generation) {
        val focus = LocalFocusManager.current
        Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 12.dp, top = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("‹", color = Pc.Accent, fontSize = 30.sp, modifier = Modifier.tap { view = View.FORM }.padding(horizontal = 8.dp))
            Text("Image generation", color = Pc.PanelTitle, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        }
        fun connect() {
            focus.clearFocus()
            val a = generationAddress(address) ?: ""
            address = a
            if (a == settings.address) g.connect() else {
                settings = settings.copy(address = a)
                keep(context)
            }
            poll(g)
        }
        Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(RowFill)) {
                Row(Modifier.fillMaxWidth().height(44.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Server", color = Ink, fontSize = 15.sp, modifier = Modifier.width(68.dp))
                    Box(Modifier.weight(1f).height(30.dp).clip(RoundedCornerShape(9.dp)).background(Field).padding(horizontal = 8.dp), contentAlignment = Alignment.CenterStart) {
                        if (address.isEmpty()) Text("192.168.1.10:8188", color = Pc.Disabled, fontSize = 14.sp)
                        BasicTextField(
                            address,
                            onValueChange = { address = it },
                            singleLine = true,
                            textStyle = TextStyle(color = Ink, fontSize = 14.sp),
                            cursorBrush = SolidColor(Pc.Accent),
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                            keyboardActions = KeyboardActions(onGo = { connect() }),
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(Pc.ActionsSeparator))
                Box(Modifier.fillMaxWidth().height(44.dp).tap { connect() }, contentAlignment = Alignment.Center) {
                    Text("Connect", color = Pc.Accent, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                }
            }
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(RowFill).padding(horizontal = 16.dp, vertical = 8.dp)) {
                for ((name, mark, colour, details) in statusLines()) {
                    Row(Modifier.fillMaxWidth().height(26.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(name, color = Ink, fontSize = 14.sp, modifier = Modifier.weight(1f))
                        Text(mark, color = colour, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    }
                    for (d in details) Text(d, color = Dim, fontSize = 12.sp, modifier = Modifier.padding(start = 12.dp, bottom = 4.dp))
                }
            }
            Row(Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(14.dp)).background(RowFill).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Largest picture", color = Ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                val current = settings.largestMegapixels ?: server?.largestMegapixels ?: 2f
                fun set(mp: Float?) { settings = settings.copy(largestMegapixels = mp); keep(context); poll(g) }
                Text("−", color = Dim, fontSize = 22.sp, modifier = Modifier.tap { set((current - 0.5f).coerceAtLeast(1f)) }.padding(horizontal = 10.dp))
                Text(
                    if (settings.largestMegapixels == null) "${megapixels(current)} MP · GPU" else "${megapixels(current)} MP",
                    color = Ink, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    modifier = Modifier.width(96.dp).tap { if (settings.largestMegapixels != null) set(null) },
                )
                Text("+", color = Dim, fontSize = 22.sp, modifier = Modifier.tap { set((current + 0.5f).coerceAtMost(16f)) }.padding(horizontal = 10.dp))
            }
            Row(
                Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(14.dp)).background(RowFill)
                    .tap { settings = settings.copy(nsfw = !settings.nsfw); keep(context) }.padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Show NSFW tags", color = Ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                Switch(settings.nsfw)
            }
            Text("Pictures and prompts go to this server and nowhere else.", color = Dim, fontSize = 12.5.sp)
            server?.licence?.takeIf { it.isNotEmpty() }?.let { Text("Anima: $it", color = Dim, fontSize = 12.5.sp) }
        }
    }

    /** Chips under the prompt: tags for the word being typed (tap to complete), else recent prompts. */
    @Composable
    private fun TagStrip(g: Generation) {
        val caret = promptField.selection.end
        val suggestions = remember(promptField.text, caret, settings.nsfw) { g.suggestTags(promptField.text, caret.toUInt()) }
        val recent = if (prompt.isEmpty()) remember(prompt) { g.recentPrompts() } else emptyList()
        if (suggestions.isEmpty() && recent.isEmpty()) return
        Row(Modifier.padding(top = 8.dp).horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            suggestions.forEachIndexed { i, t ->
                Row(
                    Modifier.height(32.dp).clip(RoundedCornerShape(10.dp)).background(RowFill)
                        .tap {
                            val done = g.completeTag(promptField.text, caret.toUInt(), i.toUInt())
                            promptField = androidx.compose.ui.text.input.TextFieldValue(done.text, androidx.compose.ui.text.TextRange(done.caret.toInt()))
                        }.padding(horizontal = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.size(7.dp).clip(CircleShape).background(kindColour(t.kind)))
                    Spacer(Modifier.width(6.dp))
                    // through an alias, it says which: "housecat → domestic cat"
                    t.alias?.let { Text("$it → ", color = Dim, fontSize = 13.sp, maxLines = 1) }
                    Text(t.tag, color = Ink, fontSize = 13.sp, maxLines = 1)
                    Spacer(Modifier.width(6.dp))
                    Text(t.count, color = Dim, fontSize = 11.sp, maxLines = 1)
                }
            }
            for (r in recent) {
                Box(Modifier.height(32.dp).widthIn(max = 220.dp).clip(RoundedCornerShape(10.dp)).background(RowFill).tap { prompt = r }.padding(horizontal = 10.dp), contentAlignment = Alignment.CenterStart) {
                    Text(r, color = Ink, fontSize = 13.sp, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                }
            }
        }
    }

    private fun kindColour(kind: io.github.artcraftandroid.procraft.core.TagKind) = when (kind) {
        io.github.artcraftandroid.procraft.core.TagKind.GENERAL -> Color(0xFF8E8E93)
        io.github.artcraftandroid.procraft.core.TagKind.ARTIST -> Color(0xFFE85D75)
        io.github.artcraftandroid.procraft.core.TagKind.COPYRIGHT -> Color(0xFFF2B440)
        io.github.artcraftandroid.procraft.core.TagKind.CHARACTER -> Color(0xFF55C98A)
        io.github.artcraftandroid.procraft.core.TagKind.META -> Color(0xFFA57BFF)
    }

    /** The small text under the resolution row: what will be made, and how. */
    private fun planText(size: SizeInfo): String {
        var words = "${size.width} × ${size.height}"
        words += when {
            size.passes > 1u -> " · ${size.passes} passes"
            size.scaledUp -> " → scaled up"
            else -> " · 1 pass"
        }
        if (size.capped) words += " · capped at ${megapixels(size.largestMegapixels)} MP"
        return words
    }

    private fun megapixels(mp: Float) = if (Math.abs(mp - Math.round(mp)) < 0.01f) "${Math.round(mp)}" else "%.1f".format(mp)

    private data class Line(val name: String, val mark: String, val colour: Color, val details: List<String>)

    private fun statusLines(): List<Line> {
        val s = server ?: return emptyList()
        return when (s.connection) {
            ServerConnection.NOT_SET -> listOf(Line("Not set up", "", OK, emptyList()))
            ServerConnection.CONNECTING -> listOf(Line("Connecting…", "", OK, emptyList()))
            ServerConnection.FAILED -> listOf(Line("Can't connect", "✕", BAD, listOfNotNull(s.problem)))
            ServerConnection.CONNECTED -> listOf(
                Line("ComfyUI", "${s.version} ✓", OK, emptyList()),
                Line("Anima models", "${s.checkpoints} found", if (s.checkpoints > 0u) OK else BAD, emptyList()),
            ) + s.features.map { f -> Line(f.label, if (f.ready) "✓" else "✕", if (f.ready) OK else BAD, f.missing.map { "needs $it" }) }
        }
    }

    @Composable
    private fun Switch(on: Boolean) {
        Box(Modifier.width(50.dp).height(30.dp).clip(RoundedCornerShape(15.dp)).background(if (on) Pc.Accent else Pc.SwitchOff), contentAlignment = if (on) Alignment.CenterEnd else Alignment.CenterStart) {
            Box(Modifier.padding(3.dp).size(24.dp).clip(CircleShape).background(Color.White))
        }
    }

    /** Whether the results bar has anything to show. */
    private fun hasResults() = state.running != null || state.results.isNotEmpty() || state.error != null

    /** The bar at the foot of the screen: the results, the run going, Again and Apply. */
    @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
    @Composable
    fun ResultsBar(vm: CanvasViewModel, modifier: Modifier) {
        if (!hasResults()) return
        val g = generation(LocalContext.current)
        val session = vm.session ?: return
        val shown = state.shown
        val target = shown ?: state.results.lastOrNull()?.id
        Row(
            modifier.padding(bottom = 22.dp).widthIn(max = 900.dp).fillMaxWidth(0.84f).height(96.dp)
                .clip(RoundedCornerShape(22.dp)).background(if (Pc.light) Color(0xF7F1F1F1) else Color(0xF0202020)).padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            val scroll = rememberScrollState()
            Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            Row(Modifier.horizontalScroll(scroll, reverseScrolling = true), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                for (r in state.results) {
                    val bitmap = bitmaps[r.id] ?: continue
                    val shape = RoundedCornerShape(10.dp)
                    Box {
                        Image(
                            bitmap, "Result", contentScale = ContentScale.Crop,
                            modifier = Modifier.size(66.dp).clip(shape)
                                .then(if (shown == r.id) Modifier.border(2.5.dp, Pc.Accent, shape) else Modifier)
                                .combinedClickable(
                                    remember { MutableInteractionSource() }, indication = null,
                                    onLongClick = { deleting = r.id },
                                    onClick = { g.showResult(session, if (shown == r.id) null else r.id) },
                                ),
                        )
                        // held: a small Delete above it, for the pictures nobody wants
                        if (deleting == r.id) {
                            val up = with(LocalDensity.current) { 48.dp.roundToPx() }
                            androidx.compose.ui.window.Popup(
                                alignment = Alignment.TopCenter,
                                offset = androidx.compose.ui.unit.IntOffset(0, -up),
                                onDismissRequest = { deleting = null },
                                properties = androidx.compose.ui.window.PopupProperties(focusable = true),
                            ) {
                                Box(
                                    Modifier.clip(RoundedCornerShape(10.dp)).background(Color(0xFFE84F51))
                                        .tap { g.deleteResult(session, r.id); deleting = null; bitmaps.remove(r.id); poll(g) }
                                        .padding(horizontal = 14.dp, vertical = 8.dp),
                                ) { Text("Delete", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
                            }
                        }
                    }
                }
                state.running?.let { run ->
                    Box(Modifier.size(66.dp).clip(RoundedCornerShape(10.dp)).background(Color(0xFF2A2A2A)).tap { g.cancel() }, contentAlignment = Alignment.Center) {
                        preview?.takeIf { run.preview != null }?.let { Image(it.second, null, contentScale = ContentScale.Crop, alpha = 0.65f, modifier = Modifier.size(66.dp)) }
                        ProgressRing(run.progress, Modifier.size(34.dp))
                    }
                }
                state.error?.let { Text(it, color = WARN, fontSize = 12.sp, maxLines = 2, modifier = Modifier.widthIn(max = 260.dp)) }
            }
            // older results off to the left: a fade and a ‹ say so (newest are at the right)
            if (scroll.value < scroll.maxValue) {
                val bar = if (Pc.light) Color(0xF7F1F1F1) else Color(0xF0202020)
                Box(
                    Modifier.width(44.dp).height(66.dp).background(androidx.compose.ui.graphics.Brush.horizontalGradient(listOf(bar, bar.copy(alpha = 0f)))),
                    contentAlignment = Alignment.CenterStart,
                ) { Text("‹", color = Ink, fontSize = 26.sp, modifier = Modifier.padding(start = 2.dp)) }
            }
            }
            state.results.firstOrNull { it.id == target }?.let { r ->
                Box(Modifier.clip(RoundedCornerShape(10.dp)).background(RowFill).tap { seed = r.seed }.padding(horizontal = 10.dp, vertical = 7.dp)) {
                    Text("seed ${r.seed}", color = Dim, fontSize = 12.sp)
                }
                if (r.scaledFromWidth > 0u) Text("scaled up from\n${r.scaledFromWidth} × ${r.scaledFromHeight}", color = Dim, fontSize = 11.sp, lineHeight = 13.sp)
            }
            Pill("Again", primary = false, enabled = last != null && state.running == null) { last?.let { run(vm, g, it) } }
            Pill("Apply", primary = true, enabled = target != null) {
                target?.let {
                    if (g.applyResult(session, it) != null) vm.showNotice("Added as a layer") else vm.showNotice("No room for another layer", warning = true)
                    // the new layer in the view-model's list
                    vm.refresh()
                }
            }
            Text("×", color = Dim, fontSize = 26.sp, modifier = Modifier.tap { g.discardResults(session); poll(g) }.padding(horizontal = 6.dp))
        }
    }

    @Composable
    private fun Pill(words: String, primary: Boolean, enabled: Boolean, onClick: () -> Unit) {
        val alpha = if (enabled) 1f else 0.4f
        Box(
            Modifier.height(38.dp).clip(RoundedCornerShape(12.dp)).background((if (primary) Pc.Accent else RowFill).let { it.copy(alpha = it.alpha * alpha) })
                .tap(enabled) { onClick() }.padding(horizontal = 16.dp),
            contentAlignment = Alignment.Center,
        ) { Text(words, color = (if (primary) Color.White else Ink).copy(alpha = alpha), fontSize = 14.sp, fontWeight = FontWeight.Medium) }
    }

    @Composable
    private fun ProgressRing(done: Float, modifier: Modifier) {
        Canvas(modifier) {
            val w = 2.4.dp.toPx()
            val r = Size(size.width - w, size.height - w)
            drawArc(Color(0x55FFFFFF), 0f, 360f, false, Offset(w / 2, w / 2), r, style = Stroke(w))
            drawArc(Pc.Accent, -90f, 360f * done.coerceIn(0.02f, 1f), false, Offset(w / 2, w / 2), r, style = Stroke(w, cap = StrokeCap.Round))
        }
    }

    private fun Modifier.tap(enabled: Boolean = true, onClick: () -> Unit): Modifier = composed {
        clickable(remember { MutableInteractionSource() }, indication = null, enabled = enabled, onClick = onClick)
    }
}
