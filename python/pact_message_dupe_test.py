from cffi import FFI
from register_ffi import get_ffi_lib
import json
import requests

ffi = FFI()
lib = get_ffi_lib(ffi) # loads the entire C namespace
lib.pactffi_logger_init()
lib.pactffi_log_to_stdout(3)
message_pact = lib.pactffi_new_pact(b'merge-test-consumer', b'merge-test-provider-message')
lib.pactffi_with_specification(message_pact, 5)
message = lib.pactffi_new_message(message_pact, b'an event indicating that an order has been created')
# lib.pactffi_message_expects_to_receive(message,b'Book (id fb5a885f-f7e8-4a50-950f-c1a64a94d500) created message')
# lib.pactffi_message_given(message, b'A book with id fb5a885f-f7e8-4a50-950f-c1a64a94d500 is required')
contents = {
        "id": {
          "pact:matcher:type": 'integer',
          "value": '1'
        }
      }
length = len(json.dumps(contents))
size = length + 1
lib.pactffi_message_with_contents(message, b'application/json', ffi.new("char[]", json.dumps(contents).encode('ascii')), size)
reified = lib.pactffi_message_reify(message)
res_write_message_pact = lib.pactffi_write_message_pact_file(message_pact, './pacts'.encode('ascii'), False)
print(res_write_message_pact)
