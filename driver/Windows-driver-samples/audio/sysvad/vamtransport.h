/*++

Module Name:

    vamtransport.h

Abstract:

    Minimal VirtualAudioMix user/kernel transport for render and capture audio bytes.

--*/

#ifndef _VAM_TRANSPORT_H_
#define _VAM_TRANSPORT_H_

#define VAM_AUDIO_TRANSPORT_DEVICE_NAME      L"\\Device\\VAMAudioTransport"
#define VAM_AUDIO_TRANSPORT_SYMBOLIC_NAME    L"\\DosDevices\\VAMAudioTransport"

#define IOCTL_VAMAUDIO_READ_RENDER \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_READ_DATA)

#define IOCTL_VAMAUDIO_GET_STATUS \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_READ_DATA)

#define IOCTL_VAMAUDIO_WRITE_CAPTURE \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_WRITE_DATA)

typedef struct _VAM_AUDIO_TRANSPORT_STATUS
{
    ULONG BufferSize;
    ULONG AvailableBytes;
    ULONGLONG TotalBytesWritten;
    ULONGLONG TotalBytesRead;
    ULONGLONG OverflowBytes;
    ULONG SampleRate;
    USHORT Channels;
    USHORT BitsPerSample;
    USHORT BlockAlign;
    ULONG CaptureBufferSize;
    ULONG CaptureAvailableBytes;
    ULONGLONG CaptureTotalBytesWritten;
    ULONGLONG CaptureTotalBytesRead;
    ULONGLONG CaptureOverflowBytes;
    ULONGLONG CaptureUnderrunBytes;
    ULONG CaptureActiveReaders;
    ULONG CaptureMaxReaderAvailableBytes;
} VAM_AUDIO_TRANSPORT_STATUS, *PVAM_AUDIO_TRANSPORT_STATUS;

NTSTATUS
VamAudioTransportInit
(
    _In_ PDRIVER_OBJECT DriverObject
);

VOID
VamAudioTransportShutdown
(
    VOID
);

VOID
VamAudioTransportWriteRender
(
    _In_reads_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount
);

VOID
VamAudioTransportSetRenderFormat
(
    _In_ PWAVEFORMATEX WaveFormat
);

VOID
VamAudioTransportReadCapture
(
    _Out_writes_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount
);

VOID
VamAudioTransportReadCaptureFormat
(
    _In_opt_ PVOID StreamContext,
    _Out_writes_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount,
    _In_ USHORT Channels,
    _In_ USHORT BitsPerSample,
    _In_ BOOLEAN IsFloat
);

VOID
VamAudioTransportCloseCaptureStream
(
    _In_opt_ PVOID StreamContext
);

DRIVER_DISPATCH VamAudioTransportDispatchCreateClose;
DRIVER_DISPATCH VamAudioTransportDispatchDeviceControl;

#endif
