use std::f64::consts::PI;

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};

const SUNSET_OFFICIAL: f64 = 90.833; // Standard sun angle for sunset
const SUNSET_NAUTICAL: f64 = 102.0; // Nautical sun angle for sunset
const SUNSET_CIVIL: f64 = 96.0; // Civil sun angle for sunset
const SUNSET_ASTRONOMICAL: f64 = 108.0; // Astronomical sun angle for sunset

pub struct SunSet {
    latitude: f64,
    longitude: f64,
    julian_date: f64,
    tz_offset: f64,
}

impl SunSet {
    // Default constructor
    pub fn new() -> SunSet {
        SunSet {
            latitude: 0.0,
            longitude: 0.0,
            julian_date: 0.0,
            tz_offset: 0.0,
        }
    }

    // Constructor with latitude, longitude, and timezone as integer
    pub fn with_lat_lon_int_tz(lat: f64, lon: f64, tz: i32) -> SunSet {
        SunSet {
            latitude: lat,
            longitude: lon,
            julian_date: 0.0,
            tz_offset: tz as f64,
        }
    }

    // Constructor with latitude, longitude, and timezone as double
    pub fn with_lat_lon_double_tz(lat: f64, lon: f64, tz: f64) -> SunSet {
        SunSet {
            latitude: lat,
            longitude: lon,
            julian_date: 0.0,
            tz_offset: tz,
        }
    }

    // Set position with integer timezone
    pub fn set_position_int_tz(&mut self, lat: f64, lon: f64, tz: i32) {
        self.latitude = lat;
        self.longitude = lon;
        self.tz_offset = if (-12..=14).contains(&tz) {
            tz as f64
        } else {
            0.0
        };
    }

    // Set position with double timezone
    pub fn set_position_double_tz(&mut self, lat: f64, lon: f64, tz: f64) {
        self.latitude = lat;
        self.longitude = lon;
        self.tz_offset = if (-12.0..=14.0).contains(&tz) {
            tz
        } else {
            0.0
        };
    }

    fn deg_to_rad(&self, angle_deg: f64) -> f64 {
        PI * angle_deg / 180.0
    }

    fn rad_to_deg(&self, angle_rad: f64) -> f64 {
        180.0 * angle_rad / PI
    }

    fn calc_mean_obliquity_of_ecliptic(&self, t: f64) -> f64 {
        let seconds = 21.448 - t * (46.8150 + t * (0.00059 - t * 0.001813));
        23.0 + (26.0 + (seconds / 60.0)) / 60.0
    }

    fn calc_geom_mean_long_sun(&self, t: f64) -> f64 {
        let l = 280.46646 + t * (36000.76983 + t * 0.0003032);
        l % 360.0
    }

    fn calc_obliquity_correction(&self, t: f64) -> f64 {
        let e0 = self.calc_mean_obliquity_of_ecliptic(t);
        let omega = 125.04 - 1934.136 * t;
        e0 + 0.00256 * (self.deg_to_rad(omega).cos())
    }

    fn calc_eccentricity_earth_orbit(&self, t: f64) -> f64 {
        0.016708634 - t * (0.000042037 + t * 0.0000001267)
    }

    fn calc_geom_mean_anomaly_sun(&self, t: f64) -> f64 {
        357.52911 + t * (35999.05029 - t * 0.0001537)
    }

    fn calc_equation_of_time(&self, t: f64) -> f64 {
        let epsilon = self.calc_obliquity_correction(t);
        let l0 = self.calc_geom_mean_long_sun(t);
        let e = self.calc_eccentricity_earth_orbit(t);
        let m = self.calc_geom_mean_anomaly_sun(t);

        let y = (self.deg_to_rad(epsilon) / 2.0).tan().powi(2);
        let sin2l0 = (2.0 * self.deg_to_rad(l0)).sin();
        let sinm = self.deg_to_rad(m).sin();
        let cos2l0 = (2.0 * self.deg_to_rad(l0)).cos();
        let sin4l0 = (4.0 * self.deg_to_rad(l0)).sin();
        let sin2m = (2.0 * self.deg_to_rad(m)).sin();

        let etime = y * sin2l0 - 2.0 * e * sinm + 4.0 * e * y * sinm * cos2l0
            - 0.5 * y * y * sin4l0
            - 1.25 * e * e * sin2m;
        self.rad_to_deg(etime) * 4.0
    }

    fn calc_time_julian_cent(&self, jd: f64) -> f64 {
        (jd - 2451545.0) / 36525.0
    }

    fn calc_sun_true_long(&self, t: f64) -> f64 {
        let l0 = self.calc_geom_mean_long_sun(t);
        let c = self.calc_sun_eq_of_center(t);
        l0 + c
    }

    fn calc_sun_apparent_long(&self, t: f64) -> f64 {
        let o = self.calc_sun_true_long(t);
        let omega = 125.04 - 1934.136 * t;
        o - 0.00569 - 0.00478 * self.deg_to_rad(omega).sin()
    }

    fn calc_sun_declination(&self, t: f64) -> f64 {
        let e = self.calc_obliquity_correction(t);
        let lambda = self.calc_sun_apparent_long(t);
        let sint = self.deg_to_rad(e).sin() * self.deg_to_rad(lambda).sin();
        self.rad_to_deg(sint.asin())
    }

    fn calc_hour_angle_sunrise(&self, lat: f64, solar_dec: f64, offset: f64) -> f64 {
        let lat_rad = self.deg_to_rad(lat);
        let sd_rad = self.deg_to_rad(solar_dec);

        (offset.to_radians().cos() / (lat_rad.cos() * sd_rad.cos()) - lat_rad.tan() * sd_rad.tan())
            .acos()
    }

    fn calc_hour_angle_sunset(&self, lat: f64, solar_dec: f64, offset: f64) -> f64 {
        -self.calc_hour_angle_sunrise(lat, solar_dec, offset)
    }

    fn calc_jd(&self, year: i32, month: i32, day: i32) -> f64 {
        let (y, m) = if month <= 2 {
            (year - 1, month + 12)
        } else {
            (year, month)
        };
        let a = (y as f64 / 100.0).floor();
        let b = 2.0 - a + (a / 4.0).floor();
        (365.25 * (y + 4716) as f64).floor() + (30.6001 * (m + 1) as f64).floor() + day as f64 + b
            - 1524.5
    }

    fn calc_jd_from_julian_cent(&self, t: f64) -> f64 {
        t * 36525.0 + 2451545.0
    }

    fn calc_sun_eq_of_center(&self, t: f64) -> f64 {
        let m = self.calc_geom_mean_anomaly_sun(t);
        let mrad = self.deg_to_rad(m);
        mrad.sin() * (1.914602 - t * (0.004817 + 0.000014 * t))
            + mrad.sin() * 2.0 * (0.019993 - 0.000101 * t)
            + mrad.sin() * 3.0 * 0.000289
    }

    fn minutes_to_midnight_to_datetime(&self, minutes: f64) -> DateTime<Utc> {
        let midnight =
            NaiveTime::from_hms_opt(0, 0, 0).expect("Could not create midnight datetime");
        let utc_midnight = Utc::now().with_time(midnight).unwrap();

        utc_midnight - Duration::minutes(minutes as i64)
    }

    fn calc_abs_sunset(&self, offset: f64) -> DateTime<Utc> {
        let t = self.calc_time_julian_cent(self.julian_date);
        // First pass to approximate sunset
        let mut eq_time = self.calc_equation_of_time(t);
        let mut solar_dec = self.calc_sun_declination(t);
        let mut hour_angle = self.calc_hour_angle_sunset(self.latitude, solar_dec, offset);
        let mut delta = self.longitude + self.rad_to_deg(hour_angle);
        let mut time_diff = 4.0 * delta; // in minutes of time
        let mut time_utc = 720.0 - time_diff - eq_time; // in minutes
        let new_t =
            self.calc_time_julian_cent(self.calc_jd_from_julian_cent(t) + time_utc / 1440.0);

        eq_time = self.calc_equation_of_time(new_t);
        solar_dec = self.calc_sun_declination(new_t);

        hour_angle = self.calc_hour_angle_sunset(self.latitude, solar_dec, offset);
        delta = self.longitude + self.rad_to_deg(hour_angle);
        time_diff = 4.0 * delta;
        time_utc = 720.0 - time_diff - eq_time; // in minutes

        self.minutes_to_midnight_to_datetime(time_utc)
    }

    pub fn calc_abs_sunrise(&self, offset: f64) -> DateTime<Utc> {
        let t = self.calc_time_julian_cent(self.julian_date);
        // First pass to approximate sunrise
        let mut eq_time = self.calc_equation_of_time(t);
        let mut solar_dec = self.calc_sun_declination(t);
        let mut hour_angle = self.calc_hour_angle_sunrise(self.latitude, solar_dec, offset);
        let mut delta = self.longitude + self.rad_to_deg(hour_angle);
        let mut time_diff = 4.0 * delta; // in minutes of time
        let mut time_utc = 720.0 - time_diff - eq_time; // in minutes
        let new_t =
            self.calc_time_julian_cent(self.calc_jd_from_julian_cent(t) + time_utc / 1440.0);

        eq_time = self.calc_equation_of_time(new_t);
        solar_dec = self.calc_sun_declination(new_t);

        hour_angle = self.calc_hour_angle_sunrise(self.latitude, solar_dec, offset);
        delta = self.longitude + self.rad_to_deg(hour_angle);
        time_diff = 4.0 * delta;
        time_utc = 720.0 - time_diff - eq_time; // in minutes

        self.minutes_to_midnight_to_datetime(time_utc)
    }

    pub fn calc_abs_sunrise_utc(&self) -> DateTime<Utc> {
        self.calc_abs_sunrise(SUNSET_OFFICIAL)
    }

    pub fn calc_abs_sunset_utc(&self) -> DateTime<Utc> {
        self.calc_abs_sunset(SUNSET_OFFICIAL)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::sunset::SunSet;

    #[test]
    fn test_sunrise() {
        let _test_timestamp = Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0);
        let expected_latitude = 35.0844;
        let expected_longitude = 106.6504;

        let sunset = SunSet::with_lat_lon_int_tz(expected_latitude, expected_longitude, 0);
        let sunrise_utc = sunset.calc_abs_sunrise_utc();

        assert_eq!(sunrise_utc, Utc::now());
    }

    #[test]
    fn test_sunset() {
        let expected_latitude = 35.0844;
        let expected_longitude = 106.6504;

        let sunset = SunSet::with_lat_lon_int_tz(expected_latitude, expected_longitude, 0);
        let sunset_utc = sunset.calc_abs_sunset_utc();

        assert_eq!(sunset_utc, Utc::now());
    }

    #[test]
    fn test_equation_of_time() {
        let expected_latitude = 35.0844;
        let expected_longitude = 106.6504;

        let sunset = SunSet::with_lat_lon_int_tz(expected_latitude, expected_longitude, 0);
        let t = sunset.calc_time_julian_cent(sunset.julian_date);
        let equation_of_time = sunset.calc_equation_of_time(t);

        assert_eq!(equation_of_time, 10.0);
    }
}
