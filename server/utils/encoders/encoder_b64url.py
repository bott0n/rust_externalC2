import base64
from urllib.parse import quote_plus, unquote

def encode(data):
	data = base64.b64encode(data)
	return quote_plus(data)[::-1]

def decode(data):
	data = unquote(data[::-1])
	return base64.b64decode(data)
