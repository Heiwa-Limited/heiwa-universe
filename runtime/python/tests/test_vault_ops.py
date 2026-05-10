import pytest
from heiwa_sidecar.handlers import op_vault_decrypt, op_vault_resolve
from heiwa_sidecar.protocol import Request, OkResponse, ErrResponse

@pytest.mark.asyncio
async def test_vault_decrypt_missing_args():
    req = Request(id="test-1", op="vault_decrypt", args={})
    res = await op_vault_decrypt(req)
    assert isinstance(res, ErrResponse)
    assert res.code == "missing_args"

@pytest.mark.asyncio
async def test_vault_resolve_missing_args():
    req = Request(id="test-2", op="vault_resolve", args={"owner_id": "1"})
    res = await op_vault_resolve(req)
    assert isinstance(res, ErrResponse)
    assert res.code == "missing_args"
    assert "db_identity" in res.message

@pytest.mark.asyncio
async def test_vault_decrypt_error_handling(monkeypatch):
    # Mock InstanceVault to raise an exception
    def mock_init(self):
        raise RuntimeError("Decryption engine failed")

    from heiwa_sdk.vault import InstanceVault
    monkeypatch.setattr(InstanceVault, "__init__", mock_init)

    req = Request(id="test-3", op="vault_decrypt", args={"ciphertext": "abc"})
    res = await op_vault_decrypt(req)
    assert isinstance(res, ErrResponse)
    assert res.code == "vault_error"
    assert "RuntimeError" in res.message
