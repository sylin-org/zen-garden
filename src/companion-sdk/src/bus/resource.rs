//! [`ResourceClass`] — what kind of physical thing the bus enumerates.

/// The enumeration domain a [`super::Device`] belongs to.
///
/// One enumerator is registered per `ResourceClass`. Adapter
/// registrations declare the class they're interested in; the bus
/// only invokes them with devices from that class.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceClass {
    /// USB serial port (CDC ACM, FTDI, CH340, CH9102, …). Optionally
    /// pre-filtered by USB vendor/product id pair so an enumerator can
    /// be scoped to a specific hardware family before identity probes
    /// run.
    UsbSerial {
        vid: Option<u16>,
        pid: Option<u16>,
    },
    // Future variants land here when the first consumer needs them:
    //   Bluetooth { service_uuid: Option<Uuid> },
    //   NetworkMdns { service_type: String },
    //   Gpio { chip: String, line: u32 },
}

impl ResourceClass {
    /// `true` when the candidate's USB vid/pid satisfy the optional
    /// filters on this `UsbSerial` class. Always `true` for non-`UsbSerial`
    /// variants paired with non-USB devices (they are filtered earlier
    /// by the per-class enumerator).
    pub fn matches_usb(&self, candidate_vid: u16, candidate_pid: u16) -> bool {
        match self {
            ResourceClass::UsbSerial { vid, pid } => {
                vid.is_none_or(|v| v == candidate_vid)
                    && pid.is_none_or(|p| p == candidate_pid)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfiltered_usb_serial_matches_anything() {
        let class = ResourceClass::UsbSerial { vid: None, pid: None };
        assert!(class.matches_usb(0x1234, 0x5678));
        assert!(class.matches_usb(0x0000, 0x0000));
    }

    #[test]
    fn vid_filter_rejects_mismatch() {
        let class = ResourceClass::UsbSerial {
            vid: Some(0x1a86),
            pid: None,
        };
        assert!(class.matches_usb(0x1a86, 0x55d4));
        assert!(!class.matches_usb(0x2e8a, 0x000a));
    }

    #[test]
    fn vid_and_pid_filter_requires_both() {
        let class = ResourceClass::UsbSerial {
            vid: Some(0x1a86),
            pid: Some(0x55d4),
        };
        assert!(class.matches_usb(0x1a86, 0x55d4));
        assert!(!class.matches_usb(0x1a86, 0x7523));
        assert!(!class.matches_usb(0x2e8a, 0x55d4));
    }
}
