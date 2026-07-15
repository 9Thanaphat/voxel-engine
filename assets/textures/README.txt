วางไฟล์ texture (PNG ขนาดแนะนำ 16x16 หรือ 32x32) ตามชื่อไฟล์ด้านล่าง
ไฟล์ไหนยังไม่มี เกมจะใช้สีพื้น (vertex color) แทนอัตโนมัติ
*** เพิ่ม/เปลี่ยนรูปแล้วต้อง restart เกม ***

dirt.png        - ดิน (ทุกด้าน + ใต้บล็อกหญ้า)
grass_top.png   - หญ้า ด้านบน
grass_side.png  - หญ้า ด้านข้าง
stone.png       - หิน
wood_top.png    - ไม้ หน้าตัดบน/ล่าง
wood_side.png   - ไม้ ด้านข้าง (เปลือก)
leaves.png      - ใบไม้
sand.png        - ทราย

รายการนี้กำหนดอยู่ในตาราง BLOCK_DEFS ใน src/voxel.rs
(เพิ่มบล็อกใหม่ = เพิ่มแถวในตาราง + enum variant + arm ใน from_u8)
