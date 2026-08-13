from heiwa_identity.node import gather_device_capabilities


def test_gather_device_capabilities_basic():
    caps = gather_device_capabilities()
    assert "vram_mb" in caps
    assert "locality" in caps
    assert "trust_tier" in caps
    assert isinstance(caps["provider_keys"], list)
    assert isinstance(caps["model_inventory"], list)
