mod state_machines;

use super::Event;
use core::marker::PhantomData;
use heapless::consts::*;
use heapless::Vec;
use lorawan_encoding::{keys::CryptoFactory, parser::DecryptedDataPayload};
use state_machines::Shared;
pub use state_machines::{no_session, session, JoinAccept};

type TimestampMs = u32;

pub struct Device<'a, R, C>
where
    R: radio::AsyncPhyRxTx + Timings,
    C: CryptoFactory + Default,
{
    state: State<'a, R>,
    crypto: PhantomData<C>,
}

pub enum State<'a, R>
where
    R: radio::AsyncPhyRxTx + Timings,
{
    NoSession(no_session::NoSession<'a, R>),
    Session(session::Session<'a, R>),
}

use core::default::Default;
impl<'a, R> State<'a, R>
where
    R: radio::AsyncPhyRxTx + Timings,
{
    fn new(shared: Shared<'a, R>) -> Self {
        State::NoSession(no_session::NoSession::new(shared))
    }
}

pub trait Timings {
    fn get_rx_window_offset_ms(&self) -> i32;
    fn get_rx_window_duration_ms(&self) -> u32;
}

#[allow(dead_code)]
impl<'a, R, C> Device<'a, R, C>
where
    R: radio::AsyncPhyRxTx + Timings + 'a,
    C: CryptoFactory + Default,
{
    pub fn new(
        region: region::Configuration,
        radio: R,
        deveui: [u8; 8],
        appeui: [u8; 8],
        appkey: [u8; 16],
        get_random: fn() -> u32,
        tx_buffer: &'a mut [u8],
    ) -> Device<'_, R, C> {
        Device {
            crypto: PhantomData::default(),
            state: State::new(Shared::new(
                radio,
                Credentials::new(appeui, deveui, appkey),
                region,
                Mac::default(),
                get_random,
                tx_buffer,
            )),
        }
    }

    pub fn get_radio(&mut self) -> &mut R {
        let shared = self.get_shared();
        shared.get_mut_radio()
    }

    pub fn get_credentials(&mut self) -> &mut Credentials {
        let shared = self.get_shared();
        shared.get_mut_credentials()
    }

    fn get_shared(&mut self) -> &mut Shared<'a, R> {
        match &mut self.state {
            State::NoSession(state) => state.get_mut_shared(),
            State::Session(state) => state.get_mut_shared(),
        }
    }

    pub fn get_datarate(&mut self) -> region::DR {
        self.get_shared().get_datarate()
    }

    pub fn set_datarate(&mut self, datarate: region::DR) {
        self.get_shared().set_datarate(datarate);
    }

    pub fn ready_to_send_data(&self) -> bool {
        matches!(&self.state, State::Session(session::Session::Idle(_)))
    }

    pub async fn send(
        self,
        data: &[u8],
        fport: u8,
        confirmed: bool,
    ) -> (Self, Result<Response, Error<R>>) {
        self.handle_event(Event::SendDataRequest(SendData {
            data,
            fport,
            confirmed,
        }))
        .await
    }

    pub fn get_fcnt_up(&self) -> Option<u32> {
        if let State::Session(session) = &self.state {
            Some(session.get_session_data().fcnt_up())
        } else {
            None
        }
    }

    pub fn get_session_keys(&self) -> Option<SessionKeys> {
        if let State::Session(session) = &self.state {
            Some(SessionKeys::copy_from_session_data(
                session.get_session_data(),
            ))
        } else {
            None
        }
    }

    pub fn take_data_downlink(&mut self) -> Option<DecryptedDataPayload<Vec<u8, U256>>> {
        self.get_shared().take_data_downlink()
    }

    pub fn take_join_accept(&mut self) -> Option<JoinAccept> {
        self.get_shared().take_join_accept()
    }

    pub async fn handle_event(self, event: Event<R>) -> (Self, Result<Response, Error<R>>) {
        match self.state {
            State::NoSession(state) => state.handle_event(event).await,
            State::Session(state) => state.handle_event(event).await,
        }
    }
}
