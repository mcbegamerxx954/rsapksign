use std::io::Cursor;

use anyhow::{Context, Result};
use apk::res::{Chunk, ResValue, ResValueType, ResXmlAttribute};
pub fn edit_manifest(
    manifest: &[u8],
    name: Option<&str>,
    pkg_name: Option<&str>,
) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(manifest);
    let Chunk::Xml(mut xchunks) = Chunk::parse(&mut cursor)? else {
        anyhow::bail!("invalid manifest 0");
    };
    let (string_pool, chunks) = xchunks.split_first_mut().unwrap();
    let Chunk::StringPool(strings, _) = string_pool else {
        anyhow::bail!("Annoying....");
    };
    // Change package name
    if let Some(pkgname) = pkg_name {
        let old_pkgname =
            edit_attr_in_element(chunks, "manifest", "package", pkgname.to_owned(), strings)?
                .with_context(|| "There is no package name in manifest.")?;

        // Get rid of conflicts
        let providers: Vec<&mut Vec<ResXmlAttribute>> = chunks
            .iter_mut()
            .filter_map(|c| parse_element(c, "provider", strings))
            .collect();

        for provider_attrs in providers {
            if let Some(value) = get_attribute_value(provider_attrs, "authorities", strings) {
                let string = &mut strings[value.data as usize];
                if let Some(suffix) = string.strip_prefix(&old_pkgname) {
                    *string = pkgname.to_owned() + suffix;
                }
            }
        }
    }
    // Editing resources.arsc is hard
    if let Some(app_name) = name {
        edit_attr_in_element(chunks, "application", "label", app_name.to_owned(), strings)?;
        edit_attr_in_element(chunks, "activity", "label", app_name.to_owned(), strings)?;
    }
    // Return modified manifest
    let mut mod_manifest = Vec::new();
    Chunk::Xml(xchunks).write(&mut Cursor::new(&mut mod_manifest))?;
    Ok(mod_manifest)
}
fn edit_attr_in_element(
    elements: &mut [Chunk],
    el_name: &str,
    attr_name: &str,
    new_str: String,
    pool: &mut Vec<String>,
) -> Result<Option<String>> {
    let attrs = elements
        .iter_mut()
        .find_map(|e| parse_element(&mut *e, el_name, pool))
        .with_context(|| format!("Xml element is missing: {el_name}"))?;
    let attr = attrs
        .iter_mut()
        .find(|a| attr_has_name(a.name, attr_name, pool))
        .with_context(|| format!("Attribute {attr_name} not found in element {el_name}"))?;

    edit_attr_string(attr, new_str, pool)
}
fn edit_attr_string(
    attr: &mut ResXmlAttribute,
    name: String,
    pool: &mut Vec<String>,
) -> Result<Option<String>> {
    let value = &mut attr.typed_value;
    let attr_type = ResValueType::from_u8(value.data_type)
        .with_context(|| format!("Type of label value is unknown: {}", value.data_type))?;
    match attr_type {
        ResValueType::String => Ok(Some(std::mem::replace(
            &mut pool[value.data as usize],
            name,
        ))),
        // In this case we overwrite it so that its a direct string, rid solving is pain
        _ => {
            let new_rvalue = ResValue {
                size: 8,
                res0: 0,
                data_type: ResValueType::String as u8,
                data: pool.len() as u32,
            };
            *value = new_rvalue;
            attr.raw_value = pool.len() as i32;
            pool.push(name);
            Ok(None)
        }
    }
}
fn get_attribute_value(attrs: &[ResXmlAttribute], name: &str, pool: &[String]) -> Option<ResValue> {
    attrs
        .iter()
        .find(|a| attr_has_name(a.name, name, pool))
        .map(|a| a.typed_value)
}
fn attr_has_name(index: i32, name: &str, string_pool: &[String]) -> bool {
    let index = match usize::try_from(index) {
        Ok(usize) => usize,
        Err(_) => return false,
    };
    string_pool.get(index).is_some_and(|s| s == name)
}
fn parse_element<'c>(
    chunk: &'c mut Chunk,
    name: &str,
    string_pool: &[String],
) -> Option<&'c mut Vec<ResXmlAttribute>> {
    let Chunk::XmlStartElement(_, el, attrs) = chunk else {
        return None;
    };
    if string_pool.get(el.name as usize).is_some_and(|s| s == name) {
        return Some(attrs);
    }
    None
}
