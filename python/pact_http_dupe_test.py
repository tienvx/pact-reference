from cffi import FFI
from register_ffi import get_ffi_lib
import json
import requests

ffi = FFI()
lib = get_ffi_lib(ffi) # loads the entire C namespace
lib.pactffi_logger_init()
lib.pactffi_log_to_stdout(3)

pact = lib.pactffi_new_pact(b'merge-test-consumer', b'merge-test-provider-http')
lib.pactffi_with_specification(pact, 5)
interaction = lib.pactffi_new_interaction(pact, b'a request for an order with an unknown ID')
lib.pactffi_with_request(interaction, b'GET', b'/api/orders/404')
lib.pactffi_with_header_v2(interaction, 0,b'Accept', 0, b'application/json')
lib.pactffi_response_status(interaction, 404)

# Start mock server
mock_server_port = lib.pactffi_create_mock_server_for_transport(pact , b'0.0.0.0',0, b'http', b'{}')
print(f"Mock server started: {mock_server_port}")

try:
    response = requests.get(f"http://127.0.0.1:{mock_server_port}/api/orders/404",
    headers={'Content-Type': 'application/json'})
    response.raise_for_status()
except requests.HTTPError as http_err:
    print(f'Client request - HTTP error occurred: {http_err}')  # Python 3.6
except Exception as err:
    print(f'Client request - Other error occurred: {err}')  # Python 3.6

result = lib.pactffi_mock_server_matched(mock_server_port)
res_write_pact = lib.pactffi_write_pact_file(mock_server_port, './pacts'.encode('ascii'), False)

## Cleanup
lib.pactffi_cleanup_mock_server(mock_server_port)
