//! Translate desktop input to the original PC control codes.
use storm_runtime::ControlInput;
use winit::{
    event::{KeyEvent, MouseButton, MouseScrollDelta},
    keyboard::{KeyCode, PhysicalKey},
};

pub(super) fn keyboard(event: &KeyEvent) -> Option<ControlInput> {
    let PhysicalKey::Code(key) = event.physical_key else {
        return None;
    };
    let pressed = event.state.is_pressed();
    if key == KeyCode::NumpadEnter {
        return Some(ControlInput::NumpadEnter(pressed));
    }
    Some(ControlInput::Button {
        code: key_code(key)?,
        pressed,
    })
}

pub(super) fn button(button: MouseButton, pressed: bool) -> Option<ControlInput> {
    let code = match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 5,
        MouseButton::Forward => 6,
        MouseButton::Other(_) => return None,
    };
    Some(ControlInput::Button { code, pressed })
}

pub(super) fn wheel(delta: MouseScrollDelta) -> i32 {
    let vertical = match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(y),
        MouseScrollDelta::PixelDelta(position) => position.y,
    };
    vertical.round() as i32
}

fn key_code(key: KeyCode) -> Option<u8> {
    use KeyCode::*;
    for (first, keys) in [
        (
            b'A',
            &[
                KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN,
                KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
            ][..],
        ),
        (
            b'0',
            &[
                Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
            ][..],
        ),
        (
            96,
            &[
                Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8,
                Numpad9,
            ][..],
        ),
        (
            112,
            &[F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12][..],
        ),
    ] {
        if let Some(offset) = keys.iter().position(|&code| code == key) {
            return Some(first + offset as u8);
        }
    }

    Some(match key {
        Backspace => 8,
        Tab => 9,
        Enter => 13,
        Pause => 19,
        CapsLock => 20,
        Escape => 27,
        Space => 32,
        PageUp => 33,
        PageDown => 34,
        End => 35,
        Home => 36,
        ArrowLeft => 37,
        ArrowUp => 38,
        ArrowRight => 39,
        ArrowDown => 40,
        PrintScreen => 44,
        Insert => 45,
        Delete => 46,
        SuperLeft => 91,
        SuperRight => 92,
        ContextMenu => 93,
        NumpadMultiply => 106,
        NumpadAdd => 107,
        NumpadComma => 108,
        NumpadSubtract => 109,
        NumpadDecimal => 110,
        NumpadDivide => 111,
        NumLock => 144,
        ScrollLock => 145,
        ShiftLeft => 160,
        ShiftRight => 161,
        ControlLeft => 162,
        ControlRight => 163,
        AltLeft => 164,
        AltRight => 165,
        Semicolon => 186,
        Equal => 187,
        Comma => 188,
        Minus => 189,
        Period => 190,
        Slash => 191,
        Backquote => 192,
        BracketLeft => 219,
        Backslash => 220,
        BracketRight => 221,
        Quote => 222,
        IntlBackslash => 226,
        _ => return None,
    })
}

#[test]
fn desktop_codes_match_the_original_pc_bindings() {
    for (key, code) in [
        (KeyCode::Escape, 27),
        (KeyCode::Space, 32),
        (KeyCode::KeyW, 87),
        (KeyCode::NumpadAdd, 107),
        (KeyCode::F12, 123),
        (KeyCode::BracketLeft, 219),
    ] {
        assert_eq!(key_code(key), Some(code));
    }
}
