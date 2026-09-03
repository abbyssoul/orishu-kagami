use serde::{Deserialize, Deserializer, Serializer, de};
use uom::si::f64::{Length, Time};
use uom::si::length::{centimeter, kilometer, meter, micrometer, millimeter, nanometer};
use uom::si::time::{day, hour, microsecond, millisecond, minute, nanosecond, second};

/// Parses a string into a value and a unit suffix.
fn parse_value_and_unit(s: &str) -> Result<(f64, &str), String> {
    let s = s.trim();
    let first_alpha = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (val_str, unit_str) = s.split_at(first_alpha);
    let val: f64 = val_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid number '{}': {}", val_str, e))?;
    Ok((val, unit_str.trim()))
}

pub fn deserialize_length<'de, D>(deserializer: D) -> Result<Length, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let (val, unit) = parse_value_and_unit(&s).map_err(de::Error::custom)?;

    match unit {
        "m" | "" => Ok(Length::new::<meter>(val)),
        "mm" => Ok(Length::new::<millimeter>(val)),
        "cm" => Ok(Length::new::<centimeter>(val)),
        "km" => Ok(Length::new::<kilometer>(val)),
        "um" | "μm" => Ok(Length::new::<micrometer>(val)),
        "nm" => Ok(Length::new::<nanometer>(val)),
        _ => Err(de::Error::custom(format!("unknown length unit: {}", unit))),
    }
}

pub fn serialize_length<S>(length: &Length, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // For simplicity, we serialize back to meters.
    // In a more complex implementation, we might want to choose the most appropriate unit.
    serializer.serialize_str(&format!("{}m", length.get::<meter>()))
}

pub fn deserialize_opt_length<'de, D>(deserializer: D) -> Result<Option<Length>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_length")] Length);

    let opt: Option<Wrapper> = Option::deserialize(deserializer)?;
    Ok(opt.map(|w| w.0))
}

pub fn serialize_opt_length<S>(length: &Option<Length>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match length {
        Some(l) => serialize_length(l, serializer),
        None => serializer.serialize_none(),
    }
}

pub fn deserialize_time<'de, D>(deserializer: D) -> Result<Time, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let (val, unit) = parse_value_and_unit(&s).map_err(de::Error::custom)?;

    match unit {
        "s" | "" => Ok(Time::new::<second>(val)),
        "ms" => Ok(Time::new::<millisecond>(val)),
        "us" | "μs" => Ok(Time::new::<microsecond>(val)),
        "ns" => Ok(Time::new::<nanosecond>(val)),
        "min" => Ok(Time::new::<minute>(val)),
        "h" => Ok(Time::new::<hour>(val)),
        "d" => Ok(Time::new::<day>(val)),
        _ => Err(de::Error::custom(format!("unknown time unit: {}", unit))),
    }
}

pub fn serialize_time<S>(time: &Time, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("{}s", time.get::<second>()))
}

pub fn deserialize_opt_time<'de, D>(deserializer: D) -> Result<Option<Time>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_time")] Time);

    let opt: Option<Wrapper> = Option::deserialize(deserializer)?;
    Ok(opt.map(|w| w.0))
}

pub fn serialize_opt_time<S>(time: &Option<Time>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match time {
        Some(t) => serialize_time(t, serializer),
        None => serializer.serialize_none(),
    }
}

pub fn deserialize_lengths<'de, D>(deserializer: D) -> Result<Vec<Length>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_length")] Length);

    let v: Vec<Wrapper> = Vec::deserialize(deserializer)?;
    Ok(v.into_iter().map(|w| w.0).collect())
}

pub fn serialize_lengths<S>(lengths: &[Length], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(lengths.len()))?;
    for l in lengths {
        seq.serialize_element(&format!("{}m", l.get::<meter>()))?;
    }
    seq.end()
}

pub fn deserialize_opt_lengths<'de, D>(deserializer: D) -> Result<Option<Vec<Length>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_lengths")] Vec<Length>);

    let opt: Option<Wrapper> = Option::deserialize(deserializer)?;
    Ok(opt.map(|w| w.0))
}

pub fn serialize_opt_lengths<S>(
    lengths: &Option<Vec<Length>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match lengths {
        Some(l) => serialize_lengths(l, serializer),
        None => serializer.serialize_none(),
    }
}
