use crate::{
    Config, Error, FaultCount, Gain, IntegrationTime, InterruptStatus, PowerSavingMode, Veml7700,
    DEVICE_ADDRESS,
};
use embedded_hal::blocking::i2c;

struct Register;
impl Register {
    const ALS_CONF: u8 = 0x00;
    const ALS_WH: u8 = 0x01;
    const ALS_WL: u8 = 0x02;
    const PSM: u8 = 0x03;
    const ALS: u8 = 0x04;
    const WHITE: u8 = 0x05;
    const ALS_INT: u8 = 0x06;
}

struct BitFlags;
impl BitFlags {
    const ALS_SD: u16 = 0x01;
    const ALS_INT_EN: u16 = 0x02;
    const PSM_EN: u16 = 0x01;
    const INT_TH_LOW: u16 = 1 << 15;
    const INT_TH_HIGH: u16 = 1 << 14;
}

impl Config {
    fn with_high(self, mask: u16) -> Self {
        Config {
            bits: self.bits | mask,
        }
    }
    fn with_low(self, mask: u16) -> Self {
        Config {
            bits: self.bits & !mask,
        }
    }
}

impl<I2C, E> Veml7700<I2C>
where
    I2C: i2c::Write<Error = E>,
{
    /// Create new instance of the VEML6040 device.
    pub fn new(i2c: I2C) -> Self {
        Veml7700 {
            i2c,
            config: Config {
                bits: BitFlags::ALS_SD,
            },
            gain: Gain::One,
            it: IntegrationTime::_100ms,
        }
    }

    /// Destroy driver instance, return I²C bus instance.
    pub fn destroy(self) -> I2C {
        self.i2c
    }
}

impl<I2C, E> Veml7700<I2C>
where
    I2C: i2c::Write<Error = E>,
{
    /// Enable the device.
    ///
    /// Note that when activating the sensor a wait time of 4 ms should be
    /// observed before the first measurement is picked up to allow for a
    /// correct start of the signal processor and oscillator.
    pub fn enable(&mut self) -> Result<(), Error<E>> {
        let config = self.config.with_low(BitFlags::ALS_SD);
        self.set_config(config)
    }

    /// Disable the device (shutdown).
    pub fn disable(&mut self) -> Result<(), Error<E>> {
        let config = self.config.with_high(BitFlags::ALS_SD);
        self.set_config(config)
    }

    /// Set the integration time.
    pub fn set_integration_time(&mut self, it: IntegrationTime) -> Result<(), Error<E>> {
        let mask = match it {
            IntegrationTime::_25ms => 0b1100,
            IntegrationTime::_50ms => 0b1000,
            IntegrationTime::_100ms => 0b0000,
            IntegrationTime::_200ms => 0b0001,
            IntegrationTime::_400ms => 0b0010,
            IntegrationTime::_800ms => 0b0011,
        };
        let config = self.config.bits & !(0b1111 << 6) | (mask << 6);
        self.set_config(Config { bits: config })?;
        self.it = it;
        Ok(())
    }

    /// Set the gain.
    pub fn set_gain(&mut self, gain: Gain) -> Result<(), Error<E>> {
        let mask = match gain {
            Gain::One => 0,
            Gain::Two => 1,
            Gain::OneEighth => 2,
            Gain::OneQuarter => 3,
        };
        let config = self.config.bits & !(0b11 << 11) | mask << 11;
        self.set_config(Config { bits: config })?;
        self.gain = gain;
        Ok(())
    }

    /// Set the number of times a threshold crossing must happen consecutively
    /// to trigger an interrupt.
    pub fn set_fault_count(&mut self, fc: FaultCount) -> Result<(), Error<E>> {
        let mask = match fc {
            FaultCount::One => 0,
            FaultCount::Two => 1,
            FaultCount::Four => 2,
            FaultCount::Eight => 3,
        };
        let config = self.config.bits & !(0b11 << 4) | mask << 4;
        self.set_config(Config { bits: config })
    }

    /// Enable interrupt generation.
    pub fn enable_interrupts(&mut self) -> Result<(), Error<E>> {
        let config = self.config.with_high(BitFlags::ALS_INT_EN);
        self.set_config(config)
    }

    /// Disable interrupt generation.
    pub fn disable_interrupts(&mut self) -> Result<(), Error<E>> {
        let config = self.config.with_low(BitFlags::ALS_INT_EN);
        self.set_config(config)
    }

    /// Set the ALS high threshold in raw format
    pub fn set_high_threshold_raw(&mut self, threshold: u16) -> Result<(), Error<E>> {
        self.write_register(Register::ALS_WH, threshold)
    }

    /// Set the ALS low threshold in raw format
    pub fn set_low_threshold_raw(&mut self, threshold: u16) -> Result<(), Error<E>> {
        self.write_register(Register::ALS_WL, threshold)
    }

    /// Enable the power-saving mode
    pub fn enable_power_saving(&mut self, psm: PowerSavingMode) -> Result<(), Error<E>> {
        let mask = match psm {
            PowerSavingMode::One => 0,
            PowerSavingMode::Two => 1,
            PowerSavingMode::Three => 2,
            PowerSavingMode::Four => 3,
        };
        let value = BitFlags::PSM_EN | mask << 1;
        self.write_register(Register::PSM, value)
    }

    /// Disable the power-saving mode
    pub fn disable_power_saving(&mut self) -> Result<(), Error<E>> {
        self.write_register(Register::PSM, 0)
    }

    fn set_config(&mut self, config: Config) -> Result<(), Error<E>> {
        self.write_register(Register::ALS_CONF, config.bits)?;
        self.config = config;
        Ok(())
    }

    fn write_register(&mut self, register: u8, value: u16) -> Result<(), Error<E>> {
        self.i2c
            .write(DEVICE_ADDRESS, &[register, value as u8, (value >> 8) as u8])
            .map_err(Error::I2C)
    }
}

impl<I2C, E> Veml7700<I2C>
where
    I2C: i2c::WriteRead<Error = E>,
{
    /// Read whether an interrupt has occurred.
    ///
    /// Note that the interrupt status is updated at the same rate as the
    /// measurements. Once triggered, flags will stay true until a measurement
    /// is taken which does not exceed the threshold.
    pub fn read_interrupt_status(&mut self) -> Result<InterruptStatus, Error<E>> {
        let data = self.read_register(Register::ALS_INT)?;
        Ok(InterruptStatus {
            was_too_low: (data & BitFlags::INT_TH_LOW) != 0,
            was_too_high: (data & BitFlags::INT_TH_HIGH) != 0,
        })
    }

    /// Read ALS high resolution output data in raw format
    pub fn read_raw(&mut self) -> Result<u16, Error<E>> {
        self.read_register(Register::ALS)
    }

    /// Read white channel measurement
    pub fn read_white(&mut self) -> Result<u16, Error<E>> {
        self.read_register(Register::WHITE)
    }

    fn read_register(&mut self, register: u8) -> Result<u16, Error<E>> {
        let mut data = [0; 2];
        self.i2c
            .write_read(DEVICE_ADDRESS, &[register], &mut data)
            .map_err(Error::I2C)
            .and(Ok(u16::from(data[0]) | u16::from(data[1]) << 8))
    }
}
