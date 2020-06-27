use crate::{
    decode::Decode,
    encode::{Encode, IsNull},
    error::BoxDynError,
    sqlite::{type_info::DataType, Sqlite, SqliteArgumentValue, SqliteTypeInfo, SqliteValueRef},
    types::Type,
    value::ValueRef,
};
use chrono::prelude::*;

impl Type<Sqlite> for NaiveDateTime {
    fn type_info() -> SqliteTypeInfo {
        SqliteTypeInfo(DataType::Datetime)
    }
}

impl Encode<'_, Sqlite> for NaiveDateTime {
    fn encode_by_ref(&self, buf: &mut Vec<SqliteArgumentValue<'_>>) -> IsNull {
        let text: String = self.format("%F %T%.f").to_string();
        Encode::<Sqlite>::encode(text, buf)
    }
}

impl<'a> Decode<'a, Sqlite> for NaiveDateTime {
    fn decode(value: SqliteValueRef<'a>) -> Result<Self, BoxDynError> {
        println!("YYY: {:?}", value.type_info());
        if let Some(type_info) = value.type_info() {
            println!("XXX: {:?}", type_info.0);
            match type_info.0 {
                DataType::Int | DataType::Int64 => {
                    return Ok(NaiveDateTime::from_timestamp(value.int64(), 0))
                }
                DataType::Float => return Ok(decode_naive_from_julian(value.double())),
                _ => (),
            }
        }
        decode_naive_from_text(value.text()?)
    }
}

fn decode_naive_from_text(text: &str) -> Result<NaiveDateTime, BoxDynError> {
    // Loop over common date time patterns, inspired by Diesel
    // https://docs.diesel.rs/src/diesel/sqlite/types/date_and_time/chrono.rs.html#56-97
    let sqlite_datetime_formats = &[
        // Most likely format
        "%F %T%.f",
        // Other formats in order of appearance in docs
        "%F %R",
        "%F %RZ",
        "%F %R%:z",
        "%F %T%.fZ",
        "%F %T%.f%:z",
        "%FT%R",
        "%FT%RZ",
        "%FT%R%:z",
        "%FT%T%.f",
        "%FT%T%.fZ",
        "%FT%T%.f%:z",
    ];

    for format in sqlite_datetime_formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(text, format) {
            return Ok(dt);
        }
    }

    return Err(err_protocol!("Did not find a matching pattern").into());
}

fn decode_naive_from_julian(julian: f64) -> NaiveDateTime {
    const UNIX_EPOCH: f64 = 2440587.5;
    const SECONDS_IN_DAY: f64 = 86400.0;
    let unix_timestamp_f = (julian - UNIX_EPOCH) * SECONDS_IN_DAY;
    let unix_timestamp = unix_timestamp_f.round() as i64;
    NaiveDateTime::from_timestamp(unix_timestamp, 0)
}

impl<Tz: TimeZone> Type<Sqlite> for DateTime<Tz> {
    fn type_info() -> SqliteTypeInfo {
        SqliteTypeInfo(DataType::Datetime)
    }
}

impl<Tz: TimeZone> Encode<'_, Sqlite> for DateTime<Tz>
where
    <Tz as TimeZone>::Offset: std::fmt::Display,
{
    fn encode_by_ref(&self, buf: &mut Vec<SqliteArgumentValue<'_>>) -> IsNull {
        let text = self.to_rfc3339();
        Encode::<Sqlite>::encode(text, buf)
    }
}

impl<'a> Decode<'a, Sqlite> for DateTime<Utc> {
    fn decode(value: SqliteValueRef<'a>) -> Result<Self, BoxDynError> {
        let text = value.text()?;
        if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
            Ok(dt.with_timezone(&Utc))
        } else {
            let dt = decode_naive_from_text(text)?;
            Ok(Utc.from_utc_datetime(&dt))
        }
    }
}

impl<'a> Decode<'a, Sqlite> for DateTime<FixedOffset> {
    fn decode(value: SqliteValueRef<'a>) -> Result<Self, BoxDynError> {
        let text = value.text()?;
        Ok(DateTime::parse_from_rfc3339(text)?)
    }
}

impl<'a> Decode<'a, Sqlite> for DateTime<Local> {
    fn decode(value: SqliteValueRef<'a>) -> Result<Self, BoxDynError> {
        let as_utc: DateTime<Utc> = Decode::<Sqlite>::decode(value)?;
        Ok(as_utc.with_timezone(&Local))
    }
}
