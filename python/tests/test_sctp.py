"""End-to-end async tests for the async-sctp wheel (real kernel SCTP loopback)."""

from __future__ import annotations

import asyncio

import pytest

import async_sctp as sctp


@pytest.mark.asyncio
async def test_one_to_one_echo():
    listener = sctp.SctpListener.bind("127.0.0.1:0")
    bound = listener.local_addr()

    async def server():
        assoc, _peer = await listener.accept()
        data, info = await assoc.recv()
        await assoc.send(data, info.stream, info.ppid)

    task = asyncio.create_task(server())
    client = await sctp.SctpAssociation.connect(bound)
    await client.send(b"hello", 2, sctp.NGAP)
    echo, info = await client.recv()
    assert echo == b"hello"
    assert info.stream == 2
    assert info.ppid == sctp.NGAP
    assert info.ppid_name() == "NGAP"
    await task


@pytest.mark.asyncio
async def test_config_streams():
    cfg = sctp.SctpConfig(out_streams=16, max_in_streams=16, nodelay=True)
    listener = sctp.SctpListener.bind("127.0.0.1:0", cfg)
    bound = listener.local_addr()

    async def server():
        assoc, _ = await listener.accept()
        data, info = await assoc.recv()
        await assoc.send(data, info.stream, info.ppid)

    task = asyncio.create_task(server())
    client = await sctp.SctpAssociation.connect(bound, cfg)
    await client.send(b"x", 10, sctp.S1AP)
    _echo, info = await client.recv()
    assert info.stream == 10
    await task


@pytest.mark.asyncio
async def test_one_to_many_and_peeloff():
    server = sctp.SctpServer.bind("127.0.0.1:0")
    bound = server.local_addr()

    client = await sctp.SctpAssociation.connect(bound)
    await client.send(b"peel", 0, sctp.M2PA)

    data, info, _addr = await server.recv()
    assert data == b"peel"
    peeled = server.peeloff(info.assoc_id)
    await peeled.send(b"reply", 0, sctp.M2PA)
    reply, _ = await client.recv()
    assert reply == b"reply"


def test_bad_address_raises():
    with pytest.raises(ValueError):
        sctp.SctpListener.bind("not-an-address")
