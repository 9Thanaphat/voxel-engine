// ท้องฟ้า procedural: gradient + ดวงอาทิตย์ + ดาว
// วาดบน skydome (ทรงกลมยักษ์ตามกล้อง) — สีคิดจากทิศมองต่อ pixel
// ผูกกับระบบเวลาเดิม: sun_dir / night_factor / สี ส่งมาจาก sky.rs (update_sky)

#import bevy_pbr::{
    mesh_functions::{get_world_from_local, mesh_position_local_to_world, mesh_position_local_to_clip},
    mesh_view_bindings::{view, globals},
}

struct SkyUniform {
    sky_top: vec4<f32>,
    sky_horizon: vec4<f32>,
    sky_bottom: vec4<f32>,
    // rgb = สีดวงอาทิตย์ (>1 ได้เพื่อให้ Bloom จับ), a = ไม่ใช้
    sun_color: vec4<f32>,
    // xyz = ทิศดวงอาทิตย์ (normalized), w = night_factor (0=กลางวัน 1=กลางคืน)
    sun_dir_night: vec4<f32>,
    // x = ขนาดดวงอาทิตย์ (cos threshold), y = ความเข้มดาว, z = มุมหมุนโดมดาว (hour_angle), w = ความยาว star trail (เรเดียน)
    params: vec4<f32>,
    // xyz = ทิศดวงจันทร์ (normalized), w = ความสว่างจันทร์ (0 กลางวัน .. 1 กลางคืน)
    moon_dir: vec4<f32>,
    // x = star density, y = size_min, z = size_max, w = twinkle_amp
    star_ctrl: vec4<f32>,
    // x = twinkle_rate_base, y = twinkle_rate_range, z = milkyway_brightness, w = moon_size (รัศมีเชิงมุม)
    star_ctrl2: vec4<f32>,
    // x = cloudiness (0..1), y = wind scroll, z = overcast/darken (0..1), w = สำรอง
    cloud_ctrl: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: SkyUniform;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
};

struct VSOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> VSOut {
    var out: VSOut;
    let world_from_local = get_world_from_local(v.instance_index);
    out.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0)).xyz;
    out.clip_position = mesh_position_local_to_clip(world_from_local, vec4<f32>(v.position, 1.0));
    return out;
}

// hash 3D -> [0,1) สำหรับ field ดาว
fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.3183099 + vec3<f32>(0.1, 0.2, 0.3));
    let r = q * 17.0;
    return fract(r.x * r.y * r.z * (r.x + r.y + r.z));
}

fn hash21(p: f32) -> vec2<f32> {
    let a = fract(p * vec2<f32>(0.1031, 0.1030));
    let b = a + dot(a, a.yx + 33.33);
    return fract((b.xx + b.yx) * b.xy);
}

// value noise 3D จาก hash3 (trilinear) + fbm สำหรับฝ้าทางช้างเผือก
fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash3(i + vec3<f32>(0.0, 0.0, 0.0));
    let c100 = hash3(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash3(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash3(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash3(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash3(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash3(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash3(i + vec3<f32>(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        v = v + a * vnoise(q);
        q = q * 2.02;
        a = a * 0.5;
    }
    return v;
}

// หมุนเวกเตอร์รอบแกน Z (ระนาบเดียวกับที่ดวงอาทิตย์/โดมดาวหมุน)
fn spin_z(v: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * v.x - s * v.y, s * v.x + c * v.y, v.z);
}

const STAR_SCALE: f32 = 220.0;

// สีดาวตามอุณหภูมิผิว (blackbody) แบบซีดๆ — ตาคนกลางคืนเห็นสีดาวจางอยู่แล้ว
// t: 0 = เย็น (แดง/ส้ม, ดาว M/K) .. 1 = ร้อน (ฟ้า-ขาว, ดาว O/B)
fn star_color(t: f32) -> vec3<f32> {
    // ไล่ ส้มอมแดง -> ขาวอมเหลือง -> ขาว -> ขาวอมฟ้า -> ฟ้า (saturation ต่ำ)
    let cool = vec3<f32>(1.00, 0.78, 0.62);
    let warm = vec3<f32>(1.00, 0.93, 0.82);
    let white = vec3<f32>(1.00, 1.00, 1.00);
    let bluew = vec3<f32>(0.82, 0.89, 1.00);
    let blue = vec3<f32>(0.68, 0.80, 1.00);
    if (t < 0.25) { return mix(cool, warm, t / 0.25); }
    if (t < 0.5)  { return mix(warm, white, (t - 0.25) / 0.25); }
    if (t < 0.75) { return mix(white, bluew, (t - 0.5) / 0.25); }
    return mix(bluew, blue, (t - 0.75) / 0.25);
}

// ดาวหนึ่ง sample: xyz = สี, w = ความสว่าง (0..1)
fn star_sample(dir: vec3<f32>, spin: f32) -> vec4<f32> {
    let c = cos(spin);
    let s = sin(spin);
    let rdir = vec3<f32>(c * dir.x - s * dir.y, s * dir.x + c * dir.y, dir.z);
    let cell = floor(rdir * STAR_SCALE);
    // เอาเฉพาะเซลล์ค่าสูง = ดาวเบาบาง ไม่รก
    let present = step(sky.star_ctrl.x, hash3(cell));
    // random ขนาด/ความสว่าง/สี ต่อดวง จาก hash คนละ offset (คงที่ต่อดวง → trail ไม่เพี้ยน)
    let r = hash3(cell + vec3<f32>(7.31, 1.73, 3.97));
    let ct = hash3(cell + vec3<f32>(2.11, 9.47, 5.23));
    let radius = mix(sky.star_ctrl.y, sky.star_ctrl.z, r * r);  // ดาวเล็กเยอะ ดาวใหญ่ไม่กี่ดวง (r²)
    let bright = mix(0.55, 1.0, r);           // ดวงใหญ่สว่างกว่านิด
    // เอียงอุณหภูมิไปทางขาว/ฟ้ามากกว่าแดง (ct² ) ให้เหมือนท้องฟ้าจริงที่ดาวแดงจัดหายาก
    let f = fract(rdir * STAR_SCALE) - 0.5;
    let b = present * smoothstep(radius, 0.0, length(f)) * bright;
    return vec4<f32>(star_color(ct * ct), b);
}

// ดาว + star trail: integrate ถอยหลังไปตาม `trail_span` (เรเดียน) เก็บ sample ที่สว่างสุด
// trail_span ∝ day_speed → เวลาปกติสั้นจนเป็นจุดกลม, เร่งเวลายิ่งเป็นเส้นยาวเหมือนถ่ายดาว
fn stars(dir: vec3<f32>, twinkle_t: f32, spin: f32, trail_span: f32) -> vec3<f32> {
    // จำนวน sample ปรับตามความยาว trail (~1 ต่อเซลล์) — เวลาปกติเหลือ 1 แซมเปิล จึงถูก
    let n = i32(clamp(trail_span * STAR_SCALE * 1.3, 1.0, 30.0));
    let denom = max(f32(n - 1), 1.0);
    var best = vec4<f32>(0.0);
    for (var i = 0; i < n; i = i + 1) {
        let k = f32(i) / denom;          // 0 = หัว (ปัจจุบัน) .. 1 = หาง (อดีต)
        let taper = 1.0 - 0.7 * k;       // หางจางลงเหมือน long-exposure ที่เพิ่งจาง
        let smp = star_sample(dir, spin - trail_span * k);
        let b = smp.w * taper;
        if (b > best.w) { best = vec4<f32>(smp.xyz, b); }
    }
    // twinkle เฉพาะตอน trail สั้น (เวลาปกติ) — พอเป็นเส้นแล้วดาวไม่ควรกะพริบ
    let h = hash3(floor(
        vec3<f32>(cos(spin) * dir.x - sin(spin) * dir.y,
                  sin(spin) * dir.x + cos(spin) * dir.y,
                  dir.z) * STAR_SCALE));
    // twinkle ช้าและเบา + แต่ละดวงจังหวะต่างกัน (rate สุ่มจาก h) ไม่กะพริบพร้อมกันเป็นพรืด
    let rate = sky.star_ctrl2.x + sky.star_ctrl2.y * h;
    let amp = sky.star_ctrl.w;
    let tw = (1.0 - amp) + amp * sin(twinkle_t * rate + h * 6.2831853);
    let twinkle_mix = clamp(1.0 - trail_span * 40.0, 0.0, 1.0);
    return best.xyz * best.w * mix(1.0, tw, twinkle_mix);
}

@fragment
fn fragment(in: VSOut) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position - view.world_position);
    let sun_dir = normalize(sky.sun_dir_night.xyz);
    let night = sky.sun_dir_night.w;

    // ---- gradient แนวตั้ง: bottom -> horizon -> top ตาม dir.y ----
    let up = clamp(dir.y, -1.0, 1.0);
    // ครึ่งบน: horizon -> top, ครึ่งล่าง: horizon -> bottom
    let t_up = pow(clamp(up, 0.0, 1.0), 0.55);
    let t_dn = pow(clamp(-up, 0.0, 1.0), 0.55);
    var col = sky.sky_horizon.rgb;
    col = mix(col, sky.sky_top.rgb, t_up);
    col = mix(col, sky.sky_bottom.rgb, t_dn);

    // ---- ดวงอาทิตย์ ----
    let cos_a = dot(dir, sun_dir);
    let sun_size = sky.params.x;
    // แกนดวง: คมตรงกลาง
    let disc = smoothstep(sun_size, sun_size + 0.0015, cos_a);
    // แสงเรืองรอบดวง (halo) กว้างกว่า จางกว่า
    let halo = pow(clamp(cos_a, 0.0, 1.0), 800.0) * 0.5;
    col += sky.sun_color.rgb * (disc + halo);

    // ---- องค์ประกอบกลางคืน ----
    let star_i = sky.params.y;
    let spin = sky.params.z;
    // extinction: ใกล้ขอบฟ้าบรรยากาศหนา ดาวจางลง + อมส้ม
    let ext = smoothstep(-0.02, 0.28, up);
    let redden = mix(vec3<f32>(1.0, 0.60, 0.40), vec3<f32>(1.0, 1.0, 1.0), smoothstep(0.0, 0.22, up));

    // ทางช้างเผือก: แถบฝ้าจางบนระนาบกาแลกซี หมุนตามโดมดาว
    if (night > 0.01) {
        let mw_axis = normalize(vec3<f32>(0.55, 0.30, 0.78));
        let rdir = spin_z(dir, spin);
        let band = smoothstep(0.72, 0.99, 1.0 - abs(dot(rdir, mw_axis)));
        let haze = fbm(rdir * 5.0);
        let mw = band * (0.10 + 0.55 * haze * haze);
        let mw_col = mix(vec3<f32>(0.55, 0.62, 0.85), vec3<f32>(0.92, 0.86, 0.80), haze);
        col += mw_col * (mw * night * ext * sky.star_ctrl2.z) * redden;
    }

    // ดาว (สีตามอุณหภูมิ + extinction ที่ขอบฟ้า)
    if (star_i > 0.001) {
        let s = stars(dir, globals.time, spin, sky.params.w);
        col += s * redden * (star_i * night * ext);
    }

    // ---- ดวงจันทร์ ----
    let moon_dir = normalize(sky.moon_dir.xyz);
    let moon_vis = sky.moon_dir.w;
    if (moon_vis > 0.001) {
        let mcos = dot(dir, moon_dir);
        let ang_r = max(sky.star_ctrl2.w, 0.005);    // รัศมีเชิงมุมของดวง (กันหารศูนย์)
        let mt = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), moon_dir));
        let mb = cross(moon_dir, mt);
        let off = dir - moon_dir * mcos;             // ระยะตั้งฉาก (แทนตำแหน่งบนผิว)
        let u = dot(off, mt) / ang_r;
        let v = dot(off, mb) / ang_r;
        let rr2 = u * u + v * v;
        let disc = smoothstep(1.0, 0.90, rr2);       // จานกลม จางที่ขอบ
        let zz = sqrt(max(0.0, 1.0 - rr2));
        let n = normalize(mt * u + mb * v + moon_dir * zz);
        // เฟส: สว่างด้านที่หันหาดวงอาทิตย์ + earthshine ด้านมืดจางๆ
        let phase = smoothstep(-0.12, 0.28, dot(n, sun_dir));
        let maria = 0.78 + 0.22 * fbm(n * 8.0);      // ทะเลจันทร์ (patch มืด)
        let moon_body = vec3<f32>(0.92, 0.94, 1.0) * disc * (0.06 + 0.94 * phase) * maria;
        let moon_halo = pow(clamp(mcos, 0.0, 1.0), 2500.0) * 0.15;
        col += (moon_body + vec3<f32>(0.70, 0.75, 0.90) * moon_halo) * moon_vis;
    }

    // ---- เมฆ: layer บนสุด (บังดวงอาทิตย์/ดาวได้) ----
    let cloudiness = sky.cloud_ctrl.x;
    if (cloudiness > 0.001 && dir.y > 0.02) {
        let wind = sky.cloud_ctrl.y;
        // project ทิศลงระนาบเมฆ (ยิ่งใกล้ขอบฟ้ายิ่งยืด) + เลื่อนตามลม
        let p = dir.xz / dir.y;
        let uv = p * 0.35 + vec2<f32>(globals.time * wind, globals.time * wind * 0.35);
        let d = fbm(vec3<f32>(uv, 0.0));
        // coverage: cloudiness ดัน threshold ให้เมฆคลุมมากขึ้น
        let cover = smoothstep(1.0 - cloudiness * 0.9 - 0.05, 1.0 - cloudiness * 0.9 + 0.20, d);
        let horizon_fade = smoothstep(0.02, 0.22, dir.y);
        let a = cover * horizon_fade;
        // สีเมฆ: รับแสงดวงอาทิตย์ (ด้านที่หันหาดวงสว่างกว่า), กลางคืนเทาเข้ม, overcast เทาลง
        let lit = clamp(dot(dir, sun_dir) * 0.5 + 0.5, 0.0, 1.0);
        let day_amt = 1.0 - night;
        var cloud_col = mix(vec3<f32>(0.55, 0.57, 0.63), vec3<f32>(1.0, 0.98, 0.95), lit)
            * (0.30 + 0.70 * day_amt);
        // แต้มสีดวงอาทิตย์ตอนเช้า/เย็นให้ขอบเมฆอมส้ม
        cloud_col += sky.sun_color.rgb * (0.04 * lit * day_amt);
        cloud_col = mix(cloud_col, vec3<f32>(0.30, 0.31, 0.36), sky.cloud_ctrl.z);
        col = mix(col, cloud_col, a);
    }

    return vec4<f32>(col, 1.0);
}
