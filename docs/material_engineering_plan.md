# Material Engineering Plan

## เป้าหมาย

ทำให้ระบบ `Crucible` และการหล่อโลหะเป็นระบบที่ผู้เล่นสามารถทดลองและควบคุมคุณภาพวัสดุได้จริง โดยผลลัพธ์ขึ้นกับสามแกนหลัก:

```text
Composition + Cooling History + Processing
                       ↓
              Material Properties
                       ↓
        พฤติกรรมของเครื่องมือและชิ้นงาน
```

ระบบภายในควรมีความลึก แต่ UI ต้องอธิบายผลลัพธ์ด้วยชื่อวัสดุ แถบคุณสมบัติ และสถานะที่เข้าใจง่าย ไม่บังคับให้ผู้เล่นอ่านสมการ metallurgy

## 1. Composition

`CrucibleData` เก็บมวลของธาตุแต่ละชนิดอยู่แล้ว จึงใช้สัดส่วนจริงเพื่อคำนวณชนิดและคุณสมบัติของโลหะได้

ผลกระทบเบื้องต้น:

- Iron สูง: ความแข็งแรงพื้นฐานดี
- Carbon ต่ำ: เหนียวและขึ้นรูปง่าย
- Carbon อยู่ในช่วงเหมาะสม: ได้ steel ที่แข็งและรักษาคม
- Carbon สูงเกินไป: แข็งแต่เปราะ
- Impurity สูง: durability และคุณภาพลดลง
- Slag ค้าง: เพิ่ม defect และลดความเหนียว
- Copper + Tin: Bronze
- Copper + Zinc: Brass

ไม่สร้าง item แยกสำหรับทุกสัดส่วน แต่จำแนกชื่อให้อ่านง่าย เช่น:

- Wrought Iron
- Low-Carbon Steel
- Medium-Carbon Steel
- High-Carbon Steel
- Cast Iron
- Impure Iron
- Bronze
- Brass
- Unknown Alloy

ข้อมูล composition จริงยังติดไปกับชิ้นงาน เพื่อรองรับการหลอมซ้ำ การตรวจสอบ และระบบขั้นสูงในอนาคต

## 2. Cooling History

เก็บประวัติทางความร้อนระหว่างการเย็นตัว โดยเริ่มจากข้อมูลขั้นต่ำ:

```rust
struct ThermalHistory {
    peak_temp: f32,
    fastest_cooling_rate: f32,
    quenched_in: Option<QuenchMedium>,
    tempered: bool,
}

enum QuenchMedium {
    Water,
    Oil,
}
```

วิธีเย็นตัวหลัก:

| วิธี | ผลโดยทั่วไป |
|---|---|
| ปล่อยเย็นในอากาศ | สมดุล แข็งปานกลางและเหนียว |
| จุ่มน้ำ | แข็งขึ้นมาก แต่เปราะและเสี่ยงร้าว |
| จุ่มน้ำมัน | แข็งขึ้นแบบควบคุม เปราะน้อยกว่าน้ำ |

การ quench จะให้ผลเฉพาะเมื่อชิ้นงานอยู่ในช่วงอุณหภูมิที่ถูกต้อง การนำชิ้นงานที่เย็นแล้วไปจุ่มจะไม่เปลี่ยนคุณสมบัติ

สถานะที่แสดงต่อผู้เล่น:

- Annealed
- Air Cooled
- Water Quenched
- Oil Quenched
- Tempered
- Overheated
- Thermally Cracked

## 3. Processing

กระบวนการหลังการหล่อทำให้ casting และ forging มีบทบาทต่างกัน:

- Cast: ผลิตง่าย แต่มี defect และความเปราะมากกว่า
- Forged: เพิ่มความเหนียวและความแข็งแรง
- Hardened: เพิ่ม hardness และ edge retention แต่ลด toughness
- Tempered: ลดความเปราะ โดยเสีย hardness เล็กน้อย
- Annealed: นิ่มและขึ้นรูปหรือซ่อมได้ง่าย

ตัวอย่างผลลัพธ์:

- Cast Iron Pickaxe: แข็งแต่เปราะ เสีย durability มากเมื่อกระแทกวัสดุแข็ง
- Forged Medium-Carbon Steel Pickaxe: สมดุล ขุดเร็วและทน
- Water-Quenched High-Carbon Steel Pickaxe: รักษาคมดีมาก แต่เสี่ยงบิ่นหรือร้าว

## 4. Material Properties

เก็บเฉพาะค่าที่มีผลกับ gameplay:

```rust
struct MaterialProperties {
    hardness: f32,
    toughness: f32,
    edge_retention: f32,
    ductility: f32,
    corrosion_resistance: f32,
    quality: f32,
}
```

- `hardness`: ระดับวัสดุที่ขุดได้และความเร็วในการทำงาน
- `toughness`: ต้านการแตกและการเสีย durability ครั้งใหญ่
- `edge_retention`: อัตราที่ความคมลดลง
- `ductility`: ความสามารถในการ forge และซ่อม
- `corrosion_resistance`: รองรับสนิมและสภาพแวดล้อมในอนาคต
- `quality`: ผลรวมของ impurity, slag, defect และความถูกต้องของกระบวนการ

สูตรควร deterministic เพื่อให้ composition และกระบวนการเดียวกันได้ผลเหมือนกันเสมอ

## 5. Player-Facing UI

UI ไม่แสดง phase diagram หรือสมการเต็ม แต่ใช้ชื่อวัสดุ สถานะ และแถบคุณสมบัติ:

```text
Medium-Carbon Steel
Tempered • Clean

Hardness       ████████
Toughness      ███████
Edge Retention ████████
Workability    ████
```

Tooltip ขั้นสูงสามารถแสดง composition เป็นเปอร์เซ็นต์สำหรับผู้เล่นที่ต้องการทดลองสูตร:

```text
Iron       98.9%
Carbon      0.7%
Impurity    0.4%
```

ต้องมี feedback ที่เชื่อมเหตุและผล เช่น:

- สีและความสว่างของโลหะตามอุณหภูมิ
- ข้อความ `Too cold to quench` หรือ `Overheated`
- เสียงและ particle ต่างกันระหว่าง quench สำเร็จกับชิ้นงานร้าว
- รายงาน defect ตอนนำชิ้นงานออกจาก mold

## 6. Casting และ Mold Integration

Flow หลัก:

```text
แร่/ส่วนผสม → Crucible → Furnace → โลหะเหลว
→ เทลง Mold → เลือกวิธีทำให้เย็น → นำชิ้นงานออก
→ Forge/Heat Treat → ประกอบเป็นเครื่องมือ
```

Mold เก็บ:

- Composition ที่รับมาจาก Crucible
- มวลโลหะ
- อุณหภูมิปัจจุบัน
- Thermal history
- สถานะ `Liquid`, `Soft`, `Solid`, `Ready`

หนึ่ง ingot ใช้มวลมาตรฐาน 1,000 กรัม Mold ที่เติมไม่เต็มยังไม่สามารถให้ชิ้นงานสมบูรณ์ได้

## 7. สิ่งที่ยังไม่ทำในเวอร์ชันแรก

- Grain structure รายเกรน
- Phase diagram เต็มรูปแบบ
- เปอร์เซ็นต์ martensite, ferrite และ pearlite แบบจำลองจริง
- การนำความร้อนระดับ voxel
- Oxygen diffusion
- หลายรอบของ normalizing และ tempering
- การจำลองความเค้นและการแตกร้าวเชิงฟิสิกส์เต็มรูปแบบ

หัวข้อเหล่านี้เพิ่มได้ภายหลังเมื่อมี gameplay ที่ใช้ผลลัพธ์อย่างชัดเจน

## 8. Implementation Roadmap

### Phase 1 — Casting Loop

- เพิ่ม Fired/Unfired Ingot Mold
- เทโลหะจาก Crucible ลง Mold
- เก็บ composition, mass และ temperature ใน Mold
- ทำ cooling state และนำ ingot ออกเมื่อเย็น
- รองรับ save และ multiplayer

### Phase 2 — Alloy Classification

- เพิ่ม normalized composition helper
- จำแนก Iron, Steel, Cast Iron, Bronze, Brass และ Unknown Alloy
- เพิ่ม unit tests บริเวณขอบช่วง composition
- แสดงชื่อวัสดุและ composition ใน tooltip

### Phase 3 — Thermal History

- บันทึก peak temperature และ cooling rate
- เพิ่ม air cooling, water quenching และ oil quenching
- ตรวจช่วงอุณหภูมิที่ quench ได้
- เพิ่มสถานะ overheated และ thermally cracked

### Phase 4 — Material Properties

- คำนวณ hardness, toughness, edge retention, ductility และ quality
- ทำสูตรให้ deterministic และมี test
- แสดงผลแบบแถบและคำอธิบายใน UI

### Phase 5 — Tool Integration

- แนบ material record ไปกับหัวเครื่องมือและเครื่องมือสำเร็จ
- ให้ hardness กำหนด mining tier/speed
- ให้ toughness และ quality ส่งผลต่อ durability damage
- ให้ edge retention ส่งผลต่อความเร็วที่ประสิทธิภาพลดลง

### Tool Prototype Model และ Material Override

เครื่องมือที่มีรูปทรงเดียวกันแต่ทำจากวัสดุต่างชนิดควรใช้โมเดลต้นแบบร่วมกัน แล้วเปลี่ยน material เฉพาะชิ้นส่วนที่เป็นโลหะ แทนการสร้าง mesh ซ้ำสำหรับทุกวัสดุ

ตัวอย่างโครงสร้าง node ของ `pickaxe_basic.gltf`:

```text
Pickaxe
├─ Handle
├─ Head
└─ Binding
```

- `Handle` ใช้ material ไม้ร่วมกัน
- `Head` เปลี่ยน material ตาม Stone, Copper, Bronze, Iron หรือ Steel
- `Binding` ใช้เชือกหรือโลหะตามเทคโนโลยีของเครื่องมือ
- Icon, viewmodel ตอนถือ, dropped item และโมเดลบนตัวผู้เล่นต้องใช้โมเดลต้นแบบและ material source เดียวกัน
- ข้อมูล gameplay เช่น composition, quality, durability, heat treatment และน้ำหนักต้องเก็บแยกจาก mesh

หลักการเลือกใช้โมเดล:

- รูปทรงเหมือนกันแต่วัสดุต่างกัน: ใช้ mesh เดิมและ override material
- รูปทรงหรือเทคโนโลยีต่างกันจริง: ใช้ mesh คนละชุด

```text
Stone Pickaxe  ─┐
Copper Pickaxe ─┼─ pickaxe_basic.gltf
Bronze Pickaxe ─┤   override Head material
Iron Pickaxe   ─┘

Steel Mining Pick ─ pickaxe_advanced.gltf
Powered Drill     ─ drill.gltf
```

คุณสมบัติของเครื่องมือคำนวณจาก material record:

- Hardness กำหนด mining tier และชนิดบล็อกที่ขุดได้
- Toughness และ quality กำหนด durability กับโอกาสแตก
- Edge retention กำหนดอัตราที่ประสิทธิภาพลดลง
- น้ำหนักกำหนดความเร็วการเหวี่ยงและ stamina cost
- Heat treatment ปรับสมดุลระหว่างความแข็งกับความเปราะ

ข้อกำหนด implementation:

- ตั้งชื่อ node ใน glTF ให้คงที่ เพื่อค้นหา `Head`, `Handle` และ `Binding` ได้อย่าง deterministic
- การ override `MeshMaterial3d` ต้องเขียนทับ material เดิมด้วย `insert` ไม่ใช้ `try_insert`
- ต้องทดสอบว่า material ถูกต้องเหมือนกันใน icon, ของที่ถือ, ของตกพื้น และโมเดลในโลก
- ไม่สร้าง item type แยกทุก composition; item เก็บ material record แล้วเลือกชื่อ สี และคุณสมบัติจากข้อมูลนั้น

### Mining Progression และ Soft Gating

ผู้เล่นควรขุดลงลึกได้ตั้งแต่ต้นโดยไม่มีข้อห้ามแบบ `tool tier < block tier` แต่โลกต้องสร้างต้นทุนที่ทำให้การพัฒนา Stone → Copper → Bronze → Iron → Steel มีความหมาย

หลักการ:

- เครื่องมือระดับต่ำยังทำลายหินหรือแร่ระดับสูงกว่าได้
- ความเร็วขุดลดลงอย่างมากเมื่อหัวเครื่องมืออ่อนกว่าวัสดุ
- durability, ความคม และรูปทรงของหัวเครื่องมือเสียหายเร็วขึ้น
- เครื่องมืออ่อนอาจทู่หรืองอ ส่วนเครื่องมือแข็งแต่เปราะอาจบิ่นหรือแตก
- การใช้เครื่องมือไม่เหมาะสมลด ore recovery และอาจทำให้แร่บางส่วนเสียหาย
- ผู้เล่นที่เตรียมตัวดีสามารถลงลึกก่อน progression ปกติได้ แต่มีต้นทุนและความเสี่ยงสูง

ความสัมพันธ์หลักไม่ควรเป็น boolean tier check แต่คำนวณจากคุณสมบัติวัสดุ:

```text
tool hardness / rock hardness
        ↓
mining speed
edge and durability damage
ore recovery
deformation or fracture risk
```

ตัวอย่างเมื่อใช้ Copper Pickaxe ขุดหินที่แข็งกว่ามาก:

- ยังขุดสำเร็จได้
- ความเร็วอาจเหลือ 10–20%
- durability และ edge retention ลดลงหลายเท่า
- หัว pickaxe มีโอกาสทู่หรืองอ
- ore recovery อาจเหลือประมาณ 30–60%

แนวทาง progression:

| เครื่องมือ | งานที่ทำได้อย่างยั่งยืน | งานที่ฝืนทำได้ |
|---|---|---|
| Stone | ดิน หินอ่อน และแร่ผิวดิน | Copper ore |
| Copper | หินทั่วไป Copper และ Tin | Iron ore และหินแข็ง |
| Bronze | หินแข็งและแร่เหล็ก | ชั้นหินลึก |
| Iron | ชั้นใต้ดินส่วนใหญ่ | แร่หายากและหินความดันสูง |
| Steel | เหมืองลึกและแร่ระดับสูง | วัสดุพิเศษ |

ความลึกต้องสร้างแรงกดดันมากกว่า hardness:

- ความมืดและการระบายอากาศ
- อุณหภูมิที่สูงขึ้น
- น้ำใต้ดินและการระบายน้ำ
- หินหลวม เพดานถล่ม และโครงค้ำเหมือง
- การขนส่งแร่กลับสู่ผิวดิน
- พลังงาน เครื่องจักร และระบบลำเลียง
- ศัตรูหรือสิ่งมีชีวิตใต้ดินในอนาคต

เป้าหมายคือไม่ห้ามผู้เล่นลงลึก แต่ทำให้เทคโนโลยีใหม่เปลี่ยนงานที่ “พอทำได้แบบเสี่ยงและสิ้นเปลือง” ให้กลายเป็นงานที่ “ทำได้อย่างมีประสิทธิภาพและยั่งยืน”

### Phase 6 — Forging และ Heat Treatment

- เพิ่ม forging state
- เพิ่ม annealing, hardening และ tempering
- ทำให้ cast และ forged parts มีข้อดีต่างกัน
- เพิ่มอุปกรณ์และ feedback ที่จำเป็น

## 9. Testing Requirements

- Conservation of mass: มวลรวมก่อนและหลังหลอม/เทต้องเท่ากัน
- Determinism: input และ process เดียวกันต้องได้ properties เดียวกัน
- Boundary tests สำหรับช่วง carbon/alloy
- Cooling-rate tests สำหรับ air, water และ oil
- Save/load round-trip ของ Crucible, Mold และชิ้นงาน
- Multiplayer host-authoritative tests สำหรับการเทและการนำชิ้นงานออก
- Invalid-action tests เช่น เทตอนเย็นเกินไป, Mold เต็ม หรือมวลไม่พอ

## Definition of Done

ระบบถือว่าเสร็จเมื่อผู้เล่นสามารถ:

1. สร้างและเผา Mold
2. หลอมส่วนผสมใน Crucible
3. เทโลหะโดยมวลไม่สูญหาย
4. เลือกวิธีทำให้เย็นและเห็นผลที่ต่างกัน
5. นำชิ้นงานไปประกอบเป็นเครื่องมือ
6. สัมผัสความแตกต่างของวัสดุผ่าน gameplay จริง
7. บันทึก โหลด และเล่น multiplayer โดยข้อมูลวัสดุไม่สูญหาย
