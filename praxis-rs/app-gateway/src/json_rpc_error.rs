use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use praxis_app_gateway_protocol::JSONRPCErrorError;

pub(crate) fn invalid_request(message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: message.into(),
        data: None,
    }
}

pub(crate) fn invalid_params(message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_PARAMS_ERROR_CODE,
        message: message.into(),
        data: None,
    }
}

pub(crate) fn internal_error(message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: message.into(),
        data: None,
    }
}
