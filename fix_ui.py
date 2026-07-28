with open('src/ui.rs', 'rb') as f:
    content = f.read()

idx = content.find(b'    });\r\n}')
if idx != -1:
    idx += 10
else:
    idx = content.find(b'    });\n}')
    if idx != -1:
        idx += 9

if idx != -1:
    new_content = content[:idx] + b'''

pub fn apply_custom_font(
    mut fonts: Query<&mut TextFont, Added<TextFont>>,
    asset_server: Res<AssetServer>,
) {
    let custom_font = asset_server.load("fonts/RD CHULAJARUEK.ttf");
    for mut text_font in &mut fonts {
        if text_font.font.id() == Handle::<Font>::default().id() {
            text_font.font = custom_font.clone();
        }
    }
}
'''
    with open('src/ui.rs', 'wb') as f:
        f.write(new_content)
