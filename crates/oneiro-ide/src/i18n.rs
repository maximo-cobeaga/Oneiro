//! Interface languages: English and Spanish. `tr("English text")` returns the Spanish text when Spanish is
//! selected (and the English text itself when it has no translation). The language is chosen in View > Language
//! and remembered; the first time it follows the language of Windows.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static SPANISH: AtomicBool = AtomicBool::new(false);

pub fn is_spanish() -> bool {
    SPANISH.load(Ordering::Relaxed)
}

pub fn set_spanish(on: bool) {
    SPANISH.store(on, Ordering::Relaxed);
    save(on);
}

fn config_file() -> Option<std::path::PathBuf> {
    crate::welcome::config_dir().map(|d| d.join("language.txt"))
}

fn save(es: bool) {
    if let Some(p) = config_file() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, if es { "es" } else { "en" });
    }
}

/// Language of the system: `ONEIRO_LANG`, then `LANG`, then (Windows) the user's locale name.
fn detect() -> bool {
    for v in ["ONEIRO_LANG", "LC_ALL", "LANG"] {
        if let Ok(s) = std::env::var(v) {
            if !s.is_empty() {
                return s.to_lowercase().starts_with("es");
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(o) = std::process::Command::new("reg").args(["query", r"HKCU\Control Panel\International", "/v", "LocaleName"]).creation_flags(0x0800_0000).output() {
            return String::from_utf8_lossy(&o.stdout).to_lowercase().contains("reg_sz    es");
        }
    }
    false
}

/// Applies the saved language (or the system's the first time).
pub fn init() {
    let saved = config_file().and_then(|p| std::fs::read_to_string(p).ok());
    let es = match saved.as_deref().map(str::trim) {
        Some("es") => true,
        Some("en") => false,
        _ => detect(),
    };
    SPANISH.store(es, Ordering::Relaxed);
}

pub fn tr(s: &'static str) -> &'static str {
    if !is_spanish() {
        return s;
    }
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| ES.iter().copied().collect()).get(s).copied().unwrap_or(s)
}

const ES: &[(&str, &str)] = &[
    ("(new, not saved yet)", "(nuevo, sin guardar)"),
    ("(none for local servers)", "(ninguna para servidores locales)"),
    ("(none)", "(ninguno)"),
    ("+ New character", "+ Nuevo personaje"),
    ("+ New episode", "+ Nuevo episodio"),
    ("+ New set", "+ Nuevo escenario"),
    ("1. Open a picture of ONE character seen from the front, standing, arms at the sides, on a plain background (or a PNG with transparency).\
2. Oneiro proposes where the head, body, arm and leg are. Drag the boxes if needed: the arm and the leg are the ones on the LEFT of the picture; the right ones are mirrored.\
3. Fix what the checker reports, name it and create the character: it is animated, lip-synced to your audio only if its face is drawn by Oneiro, and works in any style.",
        "1. Abre una imagen de UN personaje visto de frente, de pie, con los brazos a los lados, sobre fondo liso (o un PNG con transparencia).\
2. Oneiro propone dónde están la cabeza, el cuerpo, el brazo y la pierna. Arrastra las cajas si hace falta: el brazo y la pierna son los de la IZQUIERDA de la imagen; los de la derecha se reflejan.\
3. Corrige lo que indique el verificador, ponle nombre y crea el personaje: se anima, y sincroniza los labios con tu audio solo si su cara la dibuja Oneiro; funciona en cualquier estilo."),
    ("3D characters are real geometry with cel shading and ink outlines; the cape and tail are simulated in every scene.", "Los personajes 3D son geometría real con sombreado de dibujos animados y contornos de tinta; la capa y la cola se simulan en cada escena."),
    ("A vision model looks at the picture; the engine checks its boxes and sends it back to fix them. Uses the provider of the AI tab.", "Un modelo con visión mira la imagen; el motor revisa sus cajas y se las devuelve para corregirlas. Usa el proveedor de la pestaña IA."),
    ("AI", "IA"),
    ("API keys are never saved to disk: only the name of the environment variable is stored.", "Las claves de API nunca se guardan en el disco: solo se guarda el nombre de la variable de entorno."),
    ("Actions", "Acciones"),
    ("Actor labels", "Nombres de personajes"),
    ("Add to library", "Agregar a la biblioteca"),
    ("Add to the library", "Agregar a la biblioteca"),
    ("Animate (talk)", "Animar (hablar)"),
    ("Apply to script", "Aplicar al guion"),
    ("Ask the AI to fix these problems (you review the change before applying)", "Pedirle a la IA que arregle estos problemas (revisas el cambio antes de aplicarlo)"),
    ("Audio", "Audio"),
    ("Bare room (8 x 6 m, a window, a door and a table)", "Cuarto vacío (8 x 6 m, una ventana, una puerta y una mesa)"),
    ("Body", "Cuerpo"),
    ("Browse...", "Examinar..."),
    ("Camera", "Cámara"),
    ("Cancel", "Cancelar"),
    ("Cancel (Esc)", "Cancelar (Esc)"),
    ("Capitalized", "Con mayúscula"),
    ("Character", "Personaje"),
    ("Character builder", "Creador de personajes"),
    ("Characters", "Personajes"),
    ("Characters in the cast:", "Personajes del reparto:"),
    ("Choose recordings...", "Elegir grabaciones..."),
    ("Clear chat", "Limpiar chat"),
    ("Click to insert a line after the cursor.", "Haz clic para insertar una línea después del cursor."),
    ("Clone a voice (ElevenLabs)", "Clonar una voz (ElevenLabs)"),
    ("Clone voice", "Clonar voz"),
    ("Copy MCP setup command", "Copiar el comando de configuración MCP"),
    ("Copy id", "Copiar id"),
    ("Create", "Crear"),
    ("Create a sound effect", "Crear un efecto de sonido"),
    ("Create background music", "Crear música de fondo"),
    ("Create character", "Crear personaje"),
    ("Custom command (Piper, espeak-ng...)", "Comando propio (Piper, espeak-ng...)"),
    ("Cut a character out of an illustration:", "Recortar un personaje de una ilustración:"),
    ("Cutter", "Recortador"),
    ("Default engine", "Motor por defecto"),
    ("Design a character", "Diseñar un personaje"),
    ("Diner (counter, stools, clock, pie case)", "Cafetería (barra, taburetes, reloj, vitrina de tartas)"),
    ("Director", "Director"),
    ("Discard", "Descartar"),
    ("Draw it as vector parts (SVG) instead of library blocks", "Dibujarlo como partes vectoriales (SVG) en vez de bloques de la biblioteca"),
    ("Drive the selected character with WASD / arrows and turn it into script lines", "Mueve el personaje elegido con WASD / flechas y conviértelo en líneas de guion"),
    ("Edit in builder", "Editar en el creador"),
    ("Edit the .onchar text", "Editar el texto del .onchar"),
    ("Editing", "Edición"),
    ("Engines and keys: Audio tab > Voice settings.", "Motores y claves: pestaña Audio > Ajustes de voz."),
    ("Episodes", "Episodios"),
    ("Examples (changes are saved in place, copy first if you want to keep them)", "Ejemplos (los cambios se guardan en el mismo lugar; copia primero si quieres conservarlos)"),
    ("Eyes", "Ojos"),
    ("Faces and dialogue", "Caras y diálogo"),
    ("File", "Archivo"),
    ("Finds the neck, hips and torso from the silhouette", "Encuentra el cuello, las caderas y el torso a partir de la silueta"),
    ("Fit", "Ajustar"),
    ("Fix with AI", "Arreglar con IA"),
    ("Folder", "Carpeta"),
    ("Free camera", "Cámara libre"),
    ("Free camera (right-drag to orbit)", "Cámara libre (arrastra con el botón derecho para orbitar)"),
    ("Free camera from this shot", "Cámara libre desde este plano"),
    ("Generate", "Generar"),
    ("Generate missing voices automatically when I press Play or Render", "Generar las voces que falten automáticamente al presionar Reproducir o Renderizar"),
    ("Go to end (End)", "Ir al final (Fin)"),
    ("Go to start (Home)", "Ir al inicio (Inicio)"),
    ("Head", "Cabeza"),
    ("Help", "Ayuda"),
    ("Import file...", "Importar archivo..."),
    ("Import or record", "Importar o grabar"),
    ("Import...", "Importar..."),
    ("Insert", "Insertar"),
    ("Insert into script", "Insertar en el guion"),
    ("Insert it in the script at the cursor", "Insertarlo en el guion en el cursor"),
    ("Inspector", "Inspector"),
    ("Keep", "Conservar"),
    ("Keys are never saved to disk.", "Las claves nunca se guardan en el disco."),
    ("Labels", "Etiquetas"),
    ("Lets Claude Code / Codex drive this project through the Oneiro MCP server", "Permite que Claude Code / Codex manejen este proyecto mediante el servidor MCP de Oneiro"),
    ("Letters, digits and _ (starting with a letter). The first letter becomes a capital.", "Letras, números y _ (empezando con una letra). La primera letra se convierte en mayúscula."),
    ("Library", "Biblioteca"),
    ("Library file: save with Ctrl+S to reload the project.", "Archivo de la biblioteca: guarda con Ctrl+S para recargar el proyecto."),
    ("List my ElevenLabs voices", "Listar mis voces de ElevenLabs"),
    ("List voices", "Listar voces"),
    ("Listen", "Escuchar"),
    ("Look through this shot", "Mirar a través de este plano"),
    ("Loop playback", "Repetir la reproducción"),
    ("Mix timeline", "Línea de tiempo de la mezcla"),
    ("Mute", "Silenciar"),
    ("Name", "Nombre"),
    ("New project", "Nuevo proyecto"),
    ("New project...", "Nuevo proyecto..."),
    ("New project...      Ctrl+N", "Nuevo proyecto...      Ctrl+N"),
    ("Next frame (Right)", "Fotograma siguiente (Derecha)"),
    ("No changes.", "Sin cambios."),
    ("No editable shot at the playhead. Add a `camera` or `cut to` line, or click a block in the Camera row.", "No hay un plano editable en el cabezal. Agrega una línea `camera` o `cut to`, o haz clic en un bloque de la fila Cámara."),
    ("No project open", "Ningún proyecto abierto"),
    ("No project open.", "Ningún proyecto abierto."),
    ("No sounds yet. Create some below.", "Todavía no hay sonidos. Crea algunos abajo."),
    ("No sounds yet. Create some in the Audio tab.", "Todavía no hay sonidos. Crea algunos en la pestaña Audio."),
    ("Nothing recorded yet. Use give / wound / relation / remember or `Marta.mood = angry` in the script.", "Todavía no hay nada registrado. Usa give / wound / relation / remember o `Marta.mood = angry` en el guion."),
    ("Ollama: http://localhost:11434/v1   LM Studio: http://localhost:1234/v1", "Ollama: http://localhost:11434/v1   LM Studio: http://localhost:1234/v1"),
    ("Oneiro Studio - shortcuts and tips", "Oneiro Studio - atajos y consejos"),
    ("Open a project folder to start.", "Abre la carpeta de un proyecto para empezar."),
    ("Open illustration...", "Abrir ilustración..."),
    ("Open project...", "Abrir proyecto..."),
    ("Open project...     Ctrl+O", "Abrir proyecto...     Ctrl+O"),
    ("Open recent", "Abrir reciente"),
    ("Open the sound studio", "Abrir el estudio de sonido"),
    ("OpenAI-compatible TTS / local server", "TTS compatible con OpenAI / servidor local"),
    ("Orbit in the viewport, then store that view here", "Orbita en la vista y guarda ese encuadre aquí"),
    ("Output opens on first playback", "La salida de audio se abre en la primera reproducción"),
    ("Paint a scenario", "Pintar un escenario"),
    ("Pick a character to edit, or create a new one. Everything is built from library blocks - no drawing needed.", "Elige un personaje para editar, o crea uno nuevo. Todo se arma con bloques de la biblioteca: no hace falta dibujar."),
    ("Pin the free-camera view as this shot", "Fijar la vista de la cámara libre como este plano"),
    ("Play", "Reproducir"),
    ("Playback", "Reproducción"),
    ("Playhead follows editor cursor", "El cabezal sigue al cursor del editor"),
    ("Preview", "Vista previa"),
    ("Preview voice", "Probar la voz"),
    ("Previous frame (Left)", "Fotograma anterior (Izquierda)"),
    ("Problem", "Problema"),
    ("Problems", "Problemas"),
    ("Propose automatically", "Proponer automáticamente"),
    ("Propose with AI", "Proponer con IA"),
    ("Quit", "Salir"),
    ("Recent", "Recientes"),
    ("Record", "Grabar"),
    ("Record motion", "Grabar movimiento"),
    ("Record this line with your microphone", "Grabar esta línea con tu micrófono"),
    ("Recorded motion", "Movimiento grabado"),
    ("Refresh list", "Actualizar lista"),
    ("Regenerate all", "Regenerar todo"),
    ("Reject", "Rechazar"),
    ("Rendering video", "Renderizando el video"),
    ("Replaces every spoken line with a new synthesized take", "Reemplaza cada línea hablada con una toma sintetizada nueva"),
    ("Revert", "Revertir"),
    ("Save (Ctrl+S)", "Guardar (Ctrl+S)"),
    ("Save all      Ctrl+S", "Guardar todo      Ctrl+S"),
    ("Save settings", "Guardar ajustes"),
    ("Save to library", "Guardar en la biblioteca"),
    ("Script", "Guion"),
    ("Send", "Enviar"),
    ("Send a tiny request to check the connection", "Enviar una consulta mínima para probar la conexión"),
    ("Set", "Escenario"),
    ("Sets", "Escenarios"),
    ("Shortcuts and tips", "Atajos y consejos"),
    ("Shot", "Plano"),
    ("Shot camera", "Cámara del plano"),
    ("Show changes", "Mostrar cambios"),
    ("Sounds", "Sonidos"),
    ("Sounds and music", "Sonidos y música"),
    ("Space", "Espacio"),
    ("Stop", "Detener"),
    ("Story state (end of episode)", "Estado de la historia (fin del episodio)"),
    ("Style", "Estilo"),
    ("Synthesize this line with the chosen voice provider", "Sintetizar esta línea con el proveedor de voz elegido"),
    ("Test", "Probar"),
    ("The AI draws the head, body, arm and leg as SVG; the engine checks them and Oneiro animates and lip-syncs the result. Needs a capable model.", "La IA dibuja la cabeza, el cuerpo, el brazo y la pierna como SVG; el motor los revisa y Oneiro anima y sincroniza los labios del resultado. Necesita un modelo capaz."),
    ("The character barely moved - nothing to insert.", "El personaje casi no se movió: no hay nada que insertar."),
    ("The inspector appears once the script has a scene.", "El inspector aparece cuando el guion tiene una escena."),
    ("The layout looks good.", "El recorte se ve bien."),
    ("The prompt is sent on stdin; the reply is read from stdout.", "La consulta se envía por stdin; la respuesta se lee de stdout."),
    ("The puppet assembled from the pieces (it turns, walks and gestures like any character).", "El títere armado con las piezas (gira, camina y hace gestos como cualquier personaje)."),
    ("The script has no dialogue yet.", "El guion todavía no tiene diálogo."),
    ("The timeline appears once the script has a scene.", "La línea de tiempo aparece cuando el guion tiene una escena."),
    ("This character is drawn from parts (library/parts): its head, body and clothes are those drawings, so they have no block controls here. Ask the AI tab to redraw it, or edit the SVG files. Eyes, voice, style and personality still apply.", "Este personaje está dibujado con partes (library/parts): su cabeza, cuerpo y ropa son esos dibujos, por eso no tienen controles de bloques aquí. Pídele a la pestaña IA que lo redibuje, o edita los archivos SVG. Ojos, voz, estilo y personalidad sí se aplican."),
    ("Timing and sound", "Tiempo y sonido"),
    ("Title", "Título"),
    ("Undo AI change", "Deshacer el cambio de la IA"),
    ("Unpin (back to the script's placement)", "Soltar (volver a la ubicación del guion)"),
    ("Unpin (recompute from script)", "Soltar (recalcular desde el guion)"),
    ("Upload clean recordings of ONE person who agreed to it (about a minute or more; WAV, MP3...). Needs an ElevenLabs plan with voice cloning.", "Sube grabaciones limpias de UNA persona que esté de acuerdo (alrededor de un minuto o más; WAV, MP3...). Necesita un plan de ElevenLabs con clonación de voz."),
    ("Use an existing audio file (wav, mp3, ogg, flac, m4a...)", "Usar un archivo de audio existente (wav, mp3, ogg, flac, m4a...)"),
    ("Use each character's own voice", "Usar la voz propia de cada personaje"),
    ("Uses the CLI you already have installed and logged in (`claude` / `codex`), with your own subscription. Oneiro never sees your credentials.", "Usa la herramienta de línea de comandos que ya tienes instalada y con sesión iniciada (`claude` / `codex`), con tu propia suscripción. Oneiro nunca ve tus credenciales."),
    ("Uses the voice each character has in the Character builder (its provider and voice).", "Usa la voz que cada personaje tiene en el Creador de personajes (su motor y su voz)."),
    ("View", "Ver"),
    ("Viewport", "Vista"),
    ("Visual self-check (the model looks at a contact sheet of the shots; needs image support)", "Autorevisión visual (el modelo mira una hoja de contactos de los planos; requiere soporte de imágenes)"),
    ("Voice and personality", "Voz y personalidad"),
    ("Voice settings", "Ajustes de voz"),
    ("Voice settings...", "Ajustes de voz..."),
    ("Voices from", "Voces de"),
    ("Walk in place", "Caminar en el lugar"),
    ("Wardrobe", "Vestuario"),
    ("Windows speech (offline)", "Voz de Windows (sin conexión)"),
    ("Write / edit scenes", "Escribir / editar escenas"),
    ("You", "Tú"),
    ("added to the library (Sets)", "agregado a la biblioteca (Escenarios)"),
    ("applied", "aplicado"),
    ("bald", "calvo"),
    ("bars", "compases"),
    ("base URL", "URL base"),
    ("build", "construcción"),
    ("command", "comando"),
    ("cutout_2d: paper cutout - rubber_hose: 1930s noodle limbs - clay: stop-motion plasticine - ink: pen drawing - toon_3d: real 3D. A script can override it with `use character X style Y`.", "cutout_2d: recortes de papel - rubber_hose: brazos de fideo de los años 30 - clay: plastilina stop-motion - ink: dibujo a tinta - toon_3d: 3D real. Un guion puede cambiarlo con `use character X style Y`."),
    ("default voice", "voz por defecto"),
    ("dialogue", "diálogo"),
    ("edit source", "editar código"),
    ("expression", "expresión"),
    ("facing", "mirando a"),
    ("file missing", "falta el archivo"),
    ("fix the errors above first", "arregla primero los errores de arriba"),
    ("hair", "pelo"),
    ("has errors", "tiene errores"),
    ("height", "altura"),
    ("it is music", "es música"),
    ("key (this session)", "clave (esta sesión)"),
    ("key env var", "variable de la clave"),
    ("key env var (optional)", "variable de la clave (opcional)"),
    ("key variable", "variable de la clave"),
    ("lens fov", "lente fov"),
    ("letters, digits and _ (it is also the file name)", "letras, números y _ (también es el nombre del archivo)"),
    ("looks at", "mira a"),
    ("model", "modelo"),
    ("model (optional)", "modelo (opcional)"),
    ("model can see images (enables visual self-check)", "el modelo puede ver imágenes (activa la autorevisión visual)"),
    ("mouth", "boca"),
    ("move:", "movimiento:"),
    ("name", "nombre"),
    ("no audio (timing estimated)", "sin audio (tiempo estimado)"),
    ("notes for the AI", "notas para la IA"),
    ("opened in the builder (save it there)", "abierto en el creador (guárdalo allí)"),
    ("or", "o"),
    ("or key (this session only)", "o clave (solo esta sesión)"),
    ("placed by the script (edit a value or drag in the viewport to pin)", "ubicado por el guion (edita un valor o arrastra en la vista para fijarlo)"),
    ("position", "posición"),
    ("record a sound named", "grabar un sonido llamado"),
    ("rejected", "rechazado"),
    ("script has errors - showing last valid version", "el guion tiene errores: se muestra la última versión válida"),
    ("shape", "forma"),
    ("skin", "piel"),
    ("solved from the script", "calculado desde el guion"),
    ("source", "código"),
    ("start x", "x inicial"),
    ("style", "estilo"),
    ("system default", "predeterminada del sistema"),
    ("the name must start with a capital letter (letters, digits, _)", "el nombre debe empezar con mayúscula (letras, números, _)"),
    ("traits", "rasgos"),
    ("unsaved changes", "cambios sin guardar"),
    ("valid", "válido"),
    ("variation", "variación"),
    ("voice", "voz"),
    ("voice engine", "motor de voz"),
    ("voice id or name (empty: engine default)", "id o nombre de la voz (vacío: la del motor)"),
    ("wheel: zoom   shift+wheel / middle-drag: pan   click: seek / jump to script line", "rueda: zoom   shift+rueda / arrastre con botón central: desplazar   clic: ir al tiempo / saltar a la línea del guion"),
    ("write, direct and render animated series", "escribe, dirige y renderiza series animadas"),
    ("● Record motion", "● Grabar movimiento"),
    // Added with the start screen, dialogs and scenario tools.
    ("(choose)", "(elegir)"),
    ("Pause", "Pausar"),
    ("Create or open a project to start", "Crea o abre un proyecto para empezar"),
    ("New episode", "Nuevo episodio"),
    ("New set", "Nuevo escenario"),
    ("Language", "Idioma"),
    ("Clone a voice", "Clonar una voz"),
    ("Where should the project folder go?", "¿Dónde va la carpeta del proyecto?"),
    ("Starter - two characters, a diner and a short scene", "Inicial: dos personajes, una cafetería y una escena corta"),
    ("Empty - a bare room, you create the characters", "Vacío: un cuarto sin nada, tú creas los personajes"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_translates_known_texts_and_leaves_the_rest_alone() {
        SPANISH.store(true, Ordering::Relaxed);
        assert_eq!(tr("Library"), "Biblioteca");
        assert_eq!(tr("New project..."), "Nuevo proyecto...");
        assert_eq!(tr("some text nobody translated"), "some text nobody translated");
        SPANISH.store(false, Ordering::Relaxed);
        assert_eq!(tr("Library"), "Library");
    }

    #[test]
    fn the_table_has_no_duplicates_and_no_empty_translations() {
        let mut seen = std::collections::HashSet::new();
        for (en, es) in ES {
            assert!(seen.insert(*en), "duplicate key {:?}", en);
            assert!(!es.trim().is_empty(), "empty translation for {:?}", en);
        }
    }

    #[test]
    fn menus_and_main_buttons_are_covered() {
        for k in ["File", "View", "Help", "Characters", "Sets", "Episodes", "Play", "Record motion", "Save to library", "Apply to script"] {
            assert!(ES.iter().any(|(en, _)| *en == k), "missing translation for {}", k);
        }
    }
}
