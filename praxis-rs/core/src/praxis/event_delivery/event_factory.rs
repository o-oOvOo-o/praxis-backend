use praxis_protocol::protocol::DeprecationNoticeEvent;
use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::WarningEvent;

pub(crate) fn make_warning_event(id: impl Into<String>, message: impl Into<String>) -> Event {
    Event {
        id: id.into(),
        msg: EventMsg::Warning(WarningEvent {
            message: message.into(),
        }),
    }
}

pub(crate) fn make_error_event(
    id: impl Into<String>,
    message: impl Into<String>,
    praxis_error_info: Option<PraxisErrorInfo>,
) -> Event {
    Event {
        id: id.into(),
        msg: EventMsg::Error(ErrorEvent {
            message: message.into(),
            praxis_error_info,
        }),
    }
}

pub(crate) fn make_deprecation_notice_event(
    id: impl Into<String>,
    summary: impl Into<String>,
    details: Option<String>,
) -> Event {
    Event {
        id: id.into(),
        msg: EventMsg::DeprecationNotice(DeprecationNoticeEvent {
            summary: summary.into(),
            details,
        }),
    }
}
