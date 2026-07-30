# Mechanical Power, Electricity, and Factory Plan

## วิสัยทัศน์

สร้าง progression ด้านพลังงานที่ต่อเนื่องจากงานฝีมือไปสู่อุตสาหกรรม:

```text
แรงคน
→ กลไกเฟือง
→ พลังน้ำ/ลม/ไอน้ำ
→ เครื่องกำเนิดไฟฟ้า
→ มอเตอร์และระบบควบคุม
→ โรงงานอัตโนมัติ
→ อุตสาหกรรมแฟนตาซี
```

ระบบนี้ไม่ควรเป็นสำเนาของ Create mod จุดแตกต่างหลักของเกมคือชิ้นส่วนเครื่องจักรมีข้อจำกัดจากวัสดุจริง ผู้เล่นต้องหลอม ผสม หล่อ และปรับปรุงวัสดุให้เหมาะกับแรงที่เครื่องจักรต้องรับ

อ่านร่วมกับ [Material Engineering Plan](./material_engineering_plan.md)

## Design Pillars

1. เข้าใจจากการมองเห็น: การหมุน ทิศ และเครื่องที่ติดขัดต้องอ่านได้จากภาพ
2. กฎน้อยแต่ต่อยอดได้: ใช้ RPM, torque, stress และ efficiency เป็นแกน
3. วัสดุมีความหมาย: เฟืองไม้ Bronze, Cast Iron และ Steel ทำงานต่างกัน
4. เทคโนโลยีใหม่ไม่ทำให้ของเก่าหมดประโยชน์: Motor ยังขับ shaft และเครื่องจักรกลรุ่นเก่าได้
5. Automation ต้องแก้ pain point ที่ผู้เล่นเคยทำด้วยมือ ไม่ใช่ผลิตของที่ยังไม่มีเหตุผลต้องใช้
6. Simulation ต้อง deterministic และ host-authoritative เพื่อรองรับ multiplayer

## 1. Mechanical Power Model

ข้อมูลหลักของ node:

```rust
struct MechanicalNode {
    rpm: f32,
    torque: f32,
    max_stress: f32,
    efficiency: f32,
}
```

ความสัมพันธ์พื้นฐาน:

```text
Angular velocity = RPM × 2π / 60
Mechanical power = Torque × Angular velocity
```

UI ไม่จำเป็นต้องแสดงสมการ แต่แสดง:

- Speed
- Available Torque
- Load
- Stress
- Efficiency
- Rotation Direction

### กฎพื้นฐาน

```text
เฟืองเล็ก → เฟืองใหญ่
RPM ลดลง และ torque เพิ่มขึ้น

เฟืองใหญ่ → เฟืองเล็ก
RPM เพิ่มขึ้น และ torque ลดลง

โหลดต้องการ torque มากกว่าที่มี
ระบบช้าลงหรือหยุด

stress เกินขีดจำกัดของชิ้นส่วน
ชิ้นส่วนสึก เสียหาย หรือแตก
```

ไม่สร้างพลังงานจาก gear ratio การเพิ่ม torque ต้องแลกกับ RPM และสูญเสีย efficiency เล็กน้อย

## 2. Mechanical Network

ชิ้นส่วนที่ต่อถึงกันอยู่ใน mechanical network เดียวกัน ระบบต้อง:

- ตรวจ connectivity เมื่อวางหรือทุบชิ้นส่วน
- หาทิศหมุนและอัตราทดรวม
- รวมแหล่งกำลังหลายตัวอย่างปลอดภัย
- รวม load จากเครื่องจักรผู้ใช้กำลัง
- ตรวจ conflicting rotation
- cache ผลลัพธ์และคำนวณใหม่เฉพาะ topology ที่เปลี่ยน

แนวทางข้อมูล:

```rust
struct MechanicalGrid {
    networks: HashMap<MechanicalNetworkId, MechanicalNetwork>,
    node_to_network: HashMap<IVec3, MechanicalNetworkId>,
}

struct MechanicalNetwork {
    nodes: Vec<IVec3>,
    source_power: f32,
    demanded_power: f32,
    stress: f32,
    operating_speed: f32,
    fault: Option<MechanicalFault>,
}
```

ระบบไม่ควร traverse graph ทั้งโลกทุกเฟรม ให้ rebuild เฉพาะเครือข่ายที่ถูกแก้ และ update operating state ด้วย fixed timestep

## 3. Phase-One Components

เริ่มด้วยชิ้นส่วนจำนวนน้อยที่พิสูจน์ระบบครบ:

### Power Sources

- Hand Crank: กำลังต่ำ ใช้แรงผู้เล่น
- Water Wheel: กำลังต่อเนื่องตามน้ำและทิศการไหล

### Transmission

- Shaft: ส่งกำลังตามแกน
- Small Gear: เชื่อมแกนและเปลี่ยนทิศ
- Large Gear: สร้าง gear ratio
- Gearbox: เปลี่ยนแกน 90 องศาแบบอ่านง่าย

### Loads

- Millstone: บดวัตถุดิบ
- Mechanical Press: อัดแผ่นหรือขึ้นรูปชิ้นส่วน
- Generator: แปลงพลังงานกลเป็นไฟฟ้า

ยังไม่เพิ่มในเฟสแรก:

- Belt
- Chain drive
- Clutch
- Differential
- Moving contraption
- Mechanical arm
- ระบบ bearing ซับซ้อน

## 4. Material Engineering Integration

ชิ้นส่วนกลไกเก็บ material properties ที่จำเป็น:

```rust
struct MechanicalMaterial {
    max_torque: f32,
    max_rpm: f32,
    wear_resistance: f32,
    brittleness: f32,
    friction: f32,
}
```

แนวทางสมดุล:

| วัสดุ | จุดเด่น | จุดด้อย |
|---|---|---|
| Wood | ถูกและผลิตง่าย | รับ torque/RPM ต่ำ สึกเร็ว |
| Copper | friction ต่ำ | อ่อนและเสียรูปง่าย |
| Bronze | ทนการสึก เหมาะกับเฟือง | วัตถุดิบหลายชนิด |
| Cast Iron | รับแรงกดและ torque สูง | เปราะเมื่อ overload |
| Steel | รับ torque และ RPM สูง | ผลิตยากและใช้หลายกระบวนการ |
| Fantasy Alloy | คุณสมบัติพิเศษ | ใช้วัตถุดิบหายากหรือมีข้อแลกเปลี่ยน |

คุณภาพการหล่อและ heat treatment ส่งผลต่อขีดจำกัดจริง ชิ้นส่วนคุณภาพต่ำไม่ควรระเบิดแบบสุ่ม แต่ต้องมี feedback ก่อน เช่น เสียงสั่น สี stress และ particle

## 5. Wear and Failure

ช่วงแรกใช้ wear model แบบเรียบง่าย:

```text
wear rate =
base wear
× load ratio
× speed ratio
× material modifier
× lubrication modifier
```

สถานะ:

- Healthy
- Worn
- Overloaded
- Jammed
- Cracked
- Broken

หลีกเลี่ยงงานซ่อมจุกจิกเกินไป:

- wear ปกติเกิดช้า
- overload ทำให้ wear เร็วและมีคำเตือน
- catastrophic failure เกิดเมื่อฝืนต่อเนื่องหรือใช้วัสดุผิดระดับ
- lubricant และ bearing ที่ดีลด wear ใน progression ขั้นถัดไป

## 6. Mechanical Sources Progression

```text
Hand Crank
→ Water Wheel
→ Windmill
→ Steam Engine
→ Electric Motor
→ Fantasy Power Source
```

- Hand Crank: ใช้ตั้งเครื่องและงานครั้งเดียว
- Water Wheel: ผูกระบบกับแม่น้ำและ flow direction ที่เกมมีอยู่แล้ว
- Windmill: กำลังแปรผันตามสภาพอากาศ
- Steam Engine: กำลังสูงและควบคุมได้ แต่ใช้เชื้อเพลิง น้ำ และ pressure
- Electric Motor: ส่งกำลังจากระบบไฟฟ้ากลับเข้าสู่เครื่องจักรกล
- Fantasy Source: ใช้กฎเฉพาะของโลกช่วงท้ายเกม

## 7. Electricity Bridge

สองชิ้นส่วนเชื่อมโลกกลกับไฟฟ้า:

```text
Generator: Mechanical → Electrical
Motor:     Electrical → Mechanical
```

Generator:

- ต้องการ RPM ขั้นต่ำ
- มี efficiency curve
- overload เพิ่ม mechanical load
- output ขึ้นกับกำลังกลที่ป้อนจริง

Motor:

- ดึงพลังงานจาก electrical network
- มี target RPM และ torque limit
- stall แล้วกินไฟเพิ่มและเกิดความร้อน
- ใช้ขับเครื่องจักรกลเดิมหลังปลดล็อกไฟฟ้า

ระบบไฟฟ้าควรรักษาหน่วยและ model ที่มีอยู่ในโปรเจกต์ แล้วเพิ่ม adapter ระหว่าง `MechanicalGrid` กับ `ElectricalGrid` แทนการรวมสอง graph เป็นระบบเดียว

## 8. Factory Processing Chain

โรงงานเฟสแรก:

```text
Ore
→ Crusher
→ Washer/Separator
→ Crucible/Furnace
→ Mold/Casting
→ Mechanical Press
→ Finished Component
```

เครื่องจักรแต่ละตัวต้องมีเหตุผล:

- Crusher: เพิ่ม surface area หรือ yield
- Washer/Separator: ลด impurity
- Furnace: ให้ความร้อนและหลอม
- Casting: สร้าง blank หรือ ingot
- Press: สร้าง plate, gear blank หรือชิ้นส่วนมาตรฐาน

ช่วงแรกผู้เล่นขนของระหว่างเครื่องเอง เพื่อพิสูจน์ว่า processing สนุกก่อนเพิ่ม logistics

## 9. Logistics Progression

เพิ่มหลัง processing chain ใช้งานได้:

```text
Manual Carrying
→ Hopper
→ Chute
→ Conveyor
→ Filter
→ Mechanical Arm
→ Storage Routing
```

กฎ:

- Item transport แยกจาก power network
- เครื่องจักรต้องทำงานได้แม้ไม่มี automation
- Filter ใช้กฎชนิด item/composition ที่ชัดเจน
- ป้องกัน item entity จำนวนมากด้วย internal transport representation
- sync เฉพาะ state ที่ผู้เล่นจำเป็นต้องเห็น

## 10. Control and Automation

ระบบควบคุมควรต่อยอดจากไฟฟ้าที่มี:

- Switch
- Sensor
- Comparator
- Timer
- Relay
- Motor controller
- Emergency shutdown

ตัวอย่าง:

```text
Mold เต็ม
→ Sensor ส่งสัญญาณ
→ ปิด valve
→ หยุด conveyor
→ เปิด cooling system
```

ช่วงแฟนตาซีสามารถใช้ rune เป็น logic component หรือ programmable control โดยยังทำงานตามกฎ deterministic

## 11. Factory UI and Feedback

ผู้เล่นควร debug เครื่องจักรได้จากโลก ไม่ต้องเปิดหน้าต่างทุกชิ้น:

- ลูกศรหรือ animation แสดงทิศหมุน
- สี overlay แสดง load/stress
- เสียง pitch เปลี่ยนตาม RPM
- เสียงกระตุกเมื่อ torque ไม่พอ
- particle/ความร้อนเมื่อ bearing หรือ motor overload
- tooltip ตอนเล็งแสดงค่าหลัก

โหมด inspection:

```text
Network 12
Speed       48 RPM
Load        72%
Stress      Safe
Efficiency  84%
Limiting component: Wooden Shaft
```

## 12. Multiplayer and Persistence

หลักการ:

- Host คำนวณ topology, power, failure และ processing
- Client แสดง animation จาก snapshot/interpolation
- การวางและทุบส่งเป็น block edit ตาม pipeline เดิม
- ส่ง network delta เมื่อ state เปลี่ยนอย่างมีนัยสำคัญ ไม่ส่ง RPM ทุกเฟรม
- save topology-derived state เท่าที่จำเป็น และ rebuild graph หลัง load
- save wear, inventories, machine progress และ material records

ต้องมี validation สำหรับ:

- การวาง component ผิดชนิด
- network loop หรือ conflicting rotation
- client ขอแก้ state ที่ host ไม่อนุญาต
- chunk โหลดไม่ครบแต่ network ข้ามขอบ chunk
- chunk unload ระหว่างเครื่องจักรทำงาน

## 13. Performance Strategy

- ใช้ dirty topology queue
- rebuild เฉพาะ connected component ที่ได้รับผล
- fixed simulation timestep
- suspend network ที่ไม่มี player ใกล้และไม่มี process สำคัญ
- aggregate animation state ต่อ network
- หลีกเลี่ยง entity ต่อฟันเฟืองหรือ item บน conveyor
- benchmark โรงงานขนาดเล็ก กลาง และใหญ่

เป้าทดสอบเบื้องต้น:

- 100 mechanical nodes
- 1,000 mechanical nodes
- หลาย network แยกกัน
- network ข้ามหลาย chunk
- generator/motor loop

## 14. Vertical Slice

สร้าง chain เดียวให้เสร็จก่อน:

```text
แม่น้ำ
→ Water Wheel
→ Shaft
→ Small/Large Gear Ratio
→ Mechanical Press
→ Generator
→ Copper Wire
→ Lamp
```

ประสบการณ์ที่ต้องพิสูจน์:

1. ผู้เล่นมองแล้วเข้าใจทิศทางกำลัง
2. เปลี่ยนอัตราทดแล้วเห็นผลทันที
3. โหลดมากเกินไปมี feedback ที่เข้าใจได้
4. วัสดุชิ้นส่วนจำกัดเครื่องจักรอย่างยุติธรรม
5. ไฟติดครั้งแรกเป็น milestone ที่น่าจดจำ

## 15. Implementation Roadmap

### Phase 0 — Contract and Tests

- กำหนด port direction ของแต่ละ block
- กำหนดหน่วย RPM, torque, power และ stress
- กำหนดกฎ gear ratio และ efficiency
- เขียน pure-logic tests ก่อนผูก ECS/rendering

### Phase 1 — Core Mechanical Graph

- MechanicalGrid และ topology dirty queue
- Shaft, Small Gear และ Hand Crank
- ทิศหมุน อัตราทด load และ stall
- debug inspection overlay
- save/load และ chunk-boundary behavior

### Phase 2 — Material Limits

- เชื่อม material properties
- max torque/RPM
- wear, overload และ failure feedback
- repair หรือ replacement loop

### Phase 3 — Renewable Power and Loads

- Water Wheel เชื่อม river flow
- Millstone
- Mechanical Press
- recipe/process abstraction

### Phase 4 — Electrical Bridge

- Generator
- Motor
- adapter กับ ElectricalGrid
- overload, efficiency และ synchronization

### Phase 5 — Factory Processing

- Crusher
- Washer/Separator
- metallurgy processing chain
- inventories และ machine progress

### Phase 6 — Logistics

- Hopper และ Chute
- Conveyor
- Filter
- Mechanical Arm
- storage routing

### Phase 7 — Advanced Power

- Windmill
- Steam Engine
- Boiler pressure และ safety
- batteries และ motor control

### Phase 8 — Fantasy Industry

- Mana/resonance power
- exotic material components
- rune logic
- endgame machines ที่ใช้แก้เป้าหมายระดับโลก

## 16. สิ่งที่ยังไม่ควรทำ

- Moving multiblock contraptions
- รถไฟหรือยานพาหนะจาก block assembly
- fluid dynamics เต็มรูปแบบ
- gear tooth collision จริง
- stress finite-element simulation
- PLC/programming language เต็มรูปแบบ
- โรงงานขนาดมหาศาลก่อนมี benchmark

## Testing Requirements

- Conservation of power ภายใต้ gear ratio และ efficiency
- Gear direction และ ratio หลายชั้น
- Stall เมื่อ torque ไม่พอ
- Conflicting source detection
- Material stress boundary tests
- Graph split/merge เมื่อวางหรือทุบ
- Chunk boundary load/unload
- Generator และ motor conversion
- Save/load round-trip
- Host-authoritative multiplayer edits
- Deterministic simulation ที่ fixed timestep

## Definition of Done

Vertical slice ถือว่าเสร็จเมื่อผู้เล่นสามารถ:

1. สร้างชิ้นส่วนจากวัสดุที่ผลิตเอง
2. วาง Water Wheel ในแม่น้ำและรับกำลังตามการไหล
3. ส่งกำลังผ่าน shaft และเฟือง
4. เปลี่ยน gear ratio เพื่อให้เครื่องจักรทำงาน
5. ใช้ Mechanical Press ผลิตชิ้นส่วน
6. หมุน Generator และจ่ายไฟผ่านระบบไฟฟ้าเดิม
7. เปิด Lamp เป็น milestone
8. เข้าใจสาเหตุเมื่อระบบ stall หรือ overload
9. save/load และเล่น multiplayer โดย state ไม่เสีย
