pub mod daemon {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/tenon.daemon.v1.rs"));
    }
}

pub mod cp {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/tenon.cp.v1.rs"));
    }
}

pub mod egress {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/tenon.egress.v1.rs"));
    }
}

pub mod codec;
pub mod plan;
