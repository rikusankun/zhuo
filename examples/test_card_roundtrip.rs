//! 测试 PNG 角色卡读写完整流程：读取真实卡 -> 解析 -> 修改 -> 写回 -> 验证

use zhuo_card::{load_card, parse_card_bytes, write_v2_png, Card};
use std::fs;

fn main() -> anyhow::Result<()> {
    // 1. 读取真实的 SillyTavern 角色卡 PNG
    let png_path = "examples/sophie.png";
    println!("读取角色卡: {}", png_path);
    
    let card = load_card(png_path)?;
    println!("原始卡信息:");
    println!("  name: {}", card.name);
    println!("  description: {}", card.description);
    println!("  first_mes: {}", card.first_mes);
    println!("  id: {}", card.id);
    println!("  has character_book: {}", card.character_book.is_some());
    if let Some(book) = &card.character_book {
        println!("  world book name: {}", book.name);
        println!("  world book entries: {}", book.entries.len());
    }
    
    // 2. 解析原始 JSON 验证
    let png_bytes = fs::read(png_path)?;
    let raw_json = parse_card_bytes(&png_bytes)?;
    println!("\n原始 JSON 结构: spec={}, spec_version={}", 
        raw_json.get("spec").and_then(|v| v.as_str()).unwrap_or("none"),
        raw_json.get("spec_version").and_then(|v| v.as_str()).unwrap_or("none"));
    
    // 3. 修改卡片：改 name 和 description
    let mut modified_card = card.clone();
    modified_card.name = "苏菲-修改版".to_string();
    modified_card.description = "这是修改后的描述：冬堡学院的学生，改过卡片验证写回功能。".to_string();
    modified_card.first_mes = "……要来串修改版冰糖葫芦吗？我多买了两串。".to_string();
    
    println!("\n修改后的卡信息:");
    println!("  name: {}", modified_card.name);
    println!("  description: {}", modified_card.description);
    
    // 4. 写回 PNG
    let v2_json = modified_card.to_v2_json();
    let new_png_bytes = write_v2_png(&png_bytes, &v2_json)?;
    
    let output_path = "examples/sophie_modified.png";
    fs::write(output_path, &new_png_bytes)?;
    println!("\n已写回: {}", output_path);
    println!("原始大小: {} bytes, 新大小: {} bytes", png_bytes.len(), new_png_bytes.len());
    
    // 5. 验证写回的 PNG 可读且内容正确
    let verified_card = load_card(output_path)?;
    println!("\n验证写回的卡:");
    println!("  name: {}", verified_card.name);
    println!("  description: {}", verified_card.description);
    println!("  first_mes: {}", verified_card.first_mes);
    println!("  id: {}", verified_card.id);
    
    assert_eq!(verified_card.name, modified_card.name);
    assert_eq!(verified_card.description, modified_card.description);
    assert_eq!(verified_card.first_mes, modified_card.first_mes);
    println!("\n✅ PNG 角色卡读写完整流程测试通过！");
    
    // 6. 验证原始文件未被破坏（重新读取原文件）
    let original_again = load_card(png_path)?;
    assert_eq!(original_again.name, card.name);
    assert_eq!(original_again.description, card.description);
    println!("✅ 原始文件完好无损");
    
    Ok(())
}