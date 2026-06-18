#include <sysvad.h>
#include "vamtransport.h"

#define VAMAUDIO_TRANSPORT_POOLTAG 'TaVV'
#define VAMAUDIO_RENDER_BUFFER_SIZE (48000 * 2 * 4 * 2)
#define VAMAUDIO_CAPTURE_BYTES_PER_SECOND (48000 * 2 * 2)
#define VAMAUDIO_CAPTURE_BUFFER_SIZE (VAMAUDIO_CAPTURE_BYTES_PER_SECOND / 10)
#define VAMAUDIO_CAPTURE_MAX_READERS 16

typedef struct _VAM_AUDIO_CAPTURE_READER
{
    PVOID StreamContext;
    ULONG ReadOffset;
    ULONG AvailableBytes;
    BOOLEAN Active;
} VAM_AUDIO_CAPTURE_READER, *PVAM_AUDIO_CAPTURE_READER;

static PDEVICE_OBJECT g_VamAudioTransportDevice = NULL;
static UNICODE_STRING g_VamAudioTransportSymbolicName;
static PBYTE g_VamAudioRenderBuffer = NULL;
static PBYTE g_VamAudioCaptureBuffer = NULL;
static ULONG g_VamAudioRenderBufferSize = VAMAUDIO_RENDER_BUFFER_SIZE;
static ULONG g_VamAudioCaptureBufferSize = VAMAUDIO_CAPTURE_BUFFER_SIZE;
static ULONG g_VamAudioReadOffset = 0;
static ULONG g_VamAudioWriteOffset = 0;
static ULONG g_VamAudioAvailableBytes = 0;
static ULONG g_VamAudioCaptureReadOffset = 0;
static ULONG g_VamAudioCaptureWriteOffset = 0;
static ULONG g_VamAudioCaptureAvailableBytes = 0;
static VAM_AUDIO_CAPTURE_READER g_VamAudioCaptureReaders[VAMAUDIO_CAPTURE_MAX_READERS];
static ULONGLONG g_VamAudioTotalBytesWritten = 0;
static ULONGLONG g_VamAudioTotalBytesRead = 0;
static ULONGLONG g_VamAudioOverflowBytes = 0;
static ULONGLONG g_VamAudioCaptureTotalBytesWritten = 0;
static ULONGLONG g_VamAudioCaptureTotalBytesRead = 0;
static ULONGLONG g_VamAudioCaptureOverflowBytes = 0;
static ULONGLONG g_VamAudioCaptureUnderrunBytes = 0;
static ULONG g_VamAudioSampleRate = 48000;
static USHORT g_VamAudioChannels = 2;
static USHORT g_VamAudioBitsPerSample = 16;
static USHORT g_VamAudioBlockAlign = 4;
static KSPIN_LOCK g_VamAudioTransportLock;

static
VOID
VamAudioTransportCompleteIrp
(
    _Inout_ PIRP Irp,
    _In_ NTSTATUS Status,
    _In_ ULONG_PTR Information
)
{
    Irp->IoStatus.Status = Status;
    Irp->IoStatus.Information = Information;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
}

static
VOID
VamAudioTransportDropOldest
(
    _In_ ULONG ByteCount
)
{
    ULONG dropBytes = min(ByteCount, g_VamAudioAvailableBytes);

    g_VamAudioReadOffset = (g_VamAudioReadOffset + dropBytes) % g_VamAudioRenderBufferSize;
    g_VamAudioAvailableBytes -= dropBytes;
    g_VamAudioOverflowBytes += dropBytes;
}

static
PVAM_AUDIO_CAPTURE_READER
VamAudioTransportFindCaptureReaderNoLock
(
    _In_opt_ PVOID StreamContext
)
{
    PVAM_AUDIO_CAPTURE_READER freeReader = NULL;

    for (ULONG index = 0; index < VAMAUDIO_CAPTURE_MAX_READERS; index++)
    {
        PVAM_AUDIO_CAPTURE_READER reader = &g_VamAudioCaptureReaders[index];

        if (reader->Active && reader->StreamContext == StreamContext)
        {
            return reader;
        }

        if (!reader->Active && freeReader == NULL)
        {
            freeReader = reader;
        }
    }

    if (freeReader != NULL)
    {
        freeReader->StreamContext = StreamContext;
        freeReader->ReadOffset = g_VamAudioCaptureWriteOffset;
        freeReader->AvailableBytes = 0;
        freeReader->Active = TRUE;
    }

    return freeReader;
}

static
UCHAR
VamAudioTransportReadCaptureByteNoLock
(
    _Inout_ PVAM_AUDIO_CAPTURE_READER Reader
)
{
    UCHAR value = 0;

    if (Reader != NULL && Reader->AvailableBytes > 0 && g_VamAudioCaptureBuffer != NULL)
    {
        value = g_VamAudioCaptureBuffer[Reader->ReadOffset];
        Reader->ReadOffset = (Reader->ReadOffset + 1) % g_VamAudioCaptureBufferSize;
        Reader->AvailableBytes--;
        g_VamAudioCaptureTotalBytesRead++;
    }

    return value;
}

static
SHORT
VamAudioTransportReadCaptureSample16NoLock
(
    _Inout_ PVAM_AUDIO_CAPTURE_READER Reader
)
{
    UCHAR low = VamAudioTransportReadCaptureByteNoLock(Reader);
    UCHAR high = VamAudioTransportReadCaptureByteNoLock(Reader);
    return (SHORT)((USHORT)low | ((USHORT)high << 8));
}

_Must_inspect_result_
NTSTATUS
VamAudioTransportInit
(
    _In_ PDRIVER_OBJECT DriverObject
)
{
    NTSTATUS status;
    UNICODE_STRING deviceName;

    KeInitializeSpinLock(&g_VamAudioTransportLock);
    RtlInitUnicodeString(&deviceName, VAM_AUDIO_TRANSPORT_DEVICE_NAME);
    RtlInitUnicodeString(&g_VamAudioTransportSymbolicName, VAM_AUDIO_TRANSPORT_SYMBOLIC_NAME);

    g_VamAudioRenderBuffer = (PBYTE)ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        g_VamAudioRenderBufferSize,
        VAMAUDIO_TRANSPORT_POOLTAG);

    if (g_VamAudioRenderBuffer == NULL)
    {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    g_VamAudioCaptureBuffer = (PBYTE)ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        g_VamAudioCaptureBufferSize,
        VAMAUDIO_TRANSPORT_POOLTAG);

    if (g_VamAudioCaptureBuffer == NULL)
    {
        ExFreePoolWithTag(g_VamAudioRenderBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioRenderBuffer = NULL;
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    RtlZeroMemory(g_VamAudioRenderBuffer, g_VamAudioRenderBufferSize);
    RtlZeroMemory(g_VamAudioCaptureBuffer, g_VamAudioCaptureBufferSize);
    RtlZeroMemory(g_VamAudioCaptureReaders, sizeof(g_VamAudioCaptureReaders));
    g_VamAudioWriteOffset = 0;
    g_VamAudioReadOffset = 0;
    g_VamAudioAvailableBytes = 0;
    g_VamAudioCaptureWriteOffset = 0;
    g_VamAudioCaptureAvailableBytes = 0;
    g_VamAudioTotalBytesWritten = 0;
    g_VamAudioTotalBytesRead = 0;
    g_VamAudioOverflowBytes = 0;
    g_VamAudioCaptureTotalBytesWritten = 0;
    g_VamAudioCaptureTotalBytesRead = 0;
    g_VamAudioCaptureOverflowBytes = 0;
    g_VamAudioCaptureUnderrunBytes = 0;

    status = IoCreateDevice(
        DriverObject,
        0,
        &deviceName,
        FILE_DEVICE_UNKNOWN,
        FILE_DEVICE_SECURE_OPEN,
        FALSE,
        &g_VamAudioTransportDevice);

    if (!NT_SUCCESS(status))
    {
        ExFreePoolWithTag(g_VamAudioCaptureBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioCaptureBuffer = NULL;
        ExFreePoolWithTag(g_VamAudioRenderBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioRenderBuffer = NULL;
        return status;
    }

    g_VamAudioTransportDevice->Flags |= DO_BUFFERED_IO;

    status = IoCreateSymbolicLink(&g_VamAudioTransportSymbolicName, &deviceName);
    if (!NT_SUCCESS(status))
    {
        IoDeleteDevice(g_VamAudioTransportDevice);
        g_VamAudioTransportDevice = NULL;
        ExFreePoolWithTag(g_VamAudioCaptureBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioCaptureBuffer = NULL;
        ExFreePoolWithTag(g_VamAudioRenderBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioRenderBuffer = NULL;
        return status;
    }

    g_VamAudioTransportDevice->Flags &= ~DO_DEVICE_INITIALIZING;
    return STATUS_SUCCESS;
}

VOID
VamAudioTransportShutdown
(
    void
)
{
    if (g_VamAudioTransportDevice != NULL)
    {
        IoDeleteSymbolicLink(&g_VamAudioTransportSymbolicName);
        IoDeleteDevice(g_VamAudioTransportDevice);
        g_VamAudioTransportDevice = NULL;
    }

    if (g_VamAudioRenderBuffer != NULL)
    {
        ExFreePoolWithTag(g_VamAudioRenderBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioRenderBuffer = NULL;
    }

    if (g_VamAudioCaptureBuffer != NULL)
    {
        ExFreePoolWithTag(g_VamAudioCaptureBuffer, VAMAUDIO_TRANSPORT_POOLTAG);
        g_VamAudioCaptureBuffer = NULL;
    }

    g_VamAudioReadOffset = 0;
    g_VamAudioWriteOffset = 0;
    g_VamAudioAvailableBytes = 0;
    g_VamAudioCaptureReadOffset = 0;
    g_VamAudioCaptureWriteOffset = 0;
    g_VamAudioCaptureAvailableBytes = 0;
    RtlZeroMemory(g_VamAudioCaptureReaders, sizeof(g_VamAudioCaptureReaders));
    g_VamAudioTotalBytesWritten = 0;
    g_VamAudioTotalBytesRead = 0;
    g_VamAudioOverflowBytes = 0;
    g_VamAudioCaptureTotalBytesWritten = 0;
    g_VamAudioCaptureTotalBytesRead = 0;
    g_VamAudioCaptureOverflowBytes = 0;
    g_VamAudioCaptureUnderrunBytes = 0;
    g_VamAudioSampleRate = 48000;
    g_VamAudioChannels = 2;
    g_VamAudioBitsPerSample = 16;
    g_VamAudioBlockAlign = 4;
}

VOID
VamAudioTransportSetRenderFormat
(
    _In_ PWAVEFORMATEX WaveFormat
)
{
    KIRQL oldIrql;

    if (WaveFormat == NULL)
    {
        return;
    }

    KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);

    g_VamAudioSampleRate = WaveFormat->nSamplesPerSec;
    g_VamAudioChannels = WaveFormat->nChannels;
    g_VamAudioBitsPerSample = WaveFormat->wBitsPerSample;
    g_VamAudioBlockAlign = WaveFormat->nBlockAlign;

    KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);
}

VOID
VamAudioTransportReadCapture
(
    _Out_writes_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount
)
{
    VamAudioTransportReadCaptureFormat(NULL, Buffer, ByteCount, 2, 16, FALSE);
}

VOID
VamAudioTransportReadCaptureFormat
(
    _In_opt_ PVOID StreamContext,
    _Out_writes_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount,
    _In_ USHORT Channels,
    _In_ USHORT BitsPerSample,
    _In_ BOOLEAN IsFloat
)
{
    KIRQL oldIrql;
    ULONG bytesPerSample;
    ULONG outputBlockAlign;
    ULONG frameCount;
    PVAM_AUDIO_CAPTURE_READER reader;

    if (Buffer == NULL || ByteCount == 0)
    {
        return;
    }

    RtlZeroMemory(Buffer, ByteCount);

    if (g_VamAudioCaptureBuffer == NULL)
    {
        return;
    }

    bytesPerSample = BitsPerSample / 8;
    if (Channels == 0 || bytesPerSample == 0)
    {
        return;
    }

    outputBlockAlign = Channels * bytesPerSample;
    if (outputBlockAlign == 0)
    {
        return;
    }

    frameCount = ByteCount / outputBlockAlign;

    KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);
    reader = VamAudioTransportFindCaptureReaderNoLock(StreamContext);

    for (ULONG frameIndex = 0; frameIndex < frameCount; frameIndex++)
    {
        SHORT left = 0;
        SHORT right = 0;
        PBYTE frame = Buffer + (frameIndex * outputBlockAlign);

        if (reader != NULL && reader->AvailableBytes >= 4)
        {
            left = VamAudioTransportReadCaptureSample16NoLock(reader);
            right = VamAudioTransportReadCaptureSample16NoLock(reader);
        }
        else
        {
            g_VamAudioCaptureUnderrunBytes += 4;
        }

        for (USHORT channel = 0; channel < Channels; channel++)
        {
            SHORT sample16;
            PBYTE destination = frame + (channel * bytesPerSample);

            if (Channels == 1)
            {
                sample16 = (SHORT)(((LONG)left + (LONG)right) / 2);
            }
            else if (channel == 0)
            {
                sample16 = left;
            }
            else if (channel == 1)
            {
                sample16 = right;
            }
            else
            {
                sample16 = (SHORT)(((LONG)left + (LONG)right) / 2);
            }

            if (IsFloat && BitsPerSample == 32)
            {
                FLOAT sampleFloat = (FLOAT)sample16 / 32768.0f;
                RtlCopyMemory(destination, &sampleFloat, sizeof(FLOAT));
            }
            else if (BitsPerSample == 32)
            {
                LONG sample32 = ((LONG)sample16) << 16;
                RtlCopyMemory(destination, &sample32, sizeof(LONG));
            }
            else if (BitsPerSample == 16)
            {
                RtlCopyMemory(destination, &sample16, sizeof(SHORT));
            }
            else if (BitsPerSample == 8)
            {
                UCHAR sample8 = (UCHAR)((((LONG)sample16) + 32768) >> 8);
                RtlCopyMemory(destination, &sample8, sizeof(UCHAR));
            }
        }
    }

    KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);
}

VOID
VamAudioTransportCloseCaptureStream
(
    _In_opt_ PVOID StreamContext
)
{
    KIRQL oldIrql;

    if (StreamContext == NULL)
    {
        return;
    }

    KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);

    for (ULONG index = 0; index < VAMAUDIO_CAPTURE_MAX_READERS; index++)
    {
        PVAM_AUDIO_CAPTURE_READER reader = &g_VamAudioCaptureReaders[index];
        if (reader->Active && reader->StreamContext == StreamContext)
        {
            RtlZeroMemory(reader, sizeof(*reader));
            break;
        }
    }

    KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);
}

static
VOID
VamAudioTransportWriteCapture
(
    _In_reads_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount
)
{
    KIRQL oldIrql;
    ULONG firstCopyBytes;
    ULONG remainingBytes;

    if (g_VamAudioCaptureBuffer == NULL || Buffer == NULL || ByteCount == 0)
    {
        return;
    }

    if (ByteCount > g_VamAudioCaptureBufferSize)
    {
        Buffer += ByteCount - g_VamAudioCaptureBufferSize;
        g_VamAudioCaptureOverflowBytes += ByteCount - g_VamAudioCaptureBufferSize;
        ByteCount = g_VamAudioCaptureBufferSize;
    }

    KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);

    firstCopyBytes = min(ByteCount, g_VamAudioCaptureBufferSize - g_VamAudioCaptureWriteOffset);
    RtlCopyMemory(g_VamAudioCaptureBuffer + g_VamAudioCaptureWriteOffset, Buffer, firstCopyBytes);

    remainingBytes = ByteCount - firstCopyBytes;
    if (remainingBytes > 0)
    {
        RtlCopyMemory(g_VamAudioCaptureBuffer, Buffer + firstCopyBytes, remainingBytes);
    }

    g_VamAudioCaptureWriteOffset = (g_VamAudioCaptureWriteOffset + ByteCount) % g_VamAudioCaptureBufferSize;
    g_VamAudioCaptureAvailableBytes = min(g_VamAudioCaptureBufferSize, g_VamAudioCaptureAvailableBytes + ByteCount);
    g_VamAudioCaptureTotalBytesWritten += ByteCount;

    for (ULONG index = 0; index < VAMAUDIO_CAPTURE_MAX_READERS; index++)
    {
        PVAM_AUDIO_CAPTURE_READER reader = &g_VamAudioCaptureReaders[index];

        if (!reader->Active)
        {
            continue;
        }

        if (reader->AvailableBytes + ByteCount > g_VamAudioCaptureBufferSize)
        {
            ULONG dropBytes = (reader->AvailableBytes + ByteCount) - g_VamAudioCaptureBufferSize;
            reader->ReadOffset = (reader->ReadOffset + dropBytes) % g_VamAudioCaptureBufferSize;
            reader->AvailableBytes -= min(dropBytes, reader->AvailableBytes);
            g_VamAudioCaptureOverflowBytes += dropBytes;
        }

        reader->AvailableBytes += ByteCount;
    }

    KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);
}

VOID
VamAudioTransportWriteRender
(
    _In_reads_bytes_(ByteCount) PBYTE Buffer,
    _In_ ULONG ByteCount
)
{
    KIRQL oldIrql;
    ULONG freeBytes;
    ULONG firstCopyBytes;
    ULONG remainingBytes;

    if (g_VamAudioRenderBuffer == NULL || Buffer == NULL || ByteCount == 0)
    {
        return;
    }

    if (ByteCount > g_VamAudioRenderBufferSize)
    {
        Buffer += ByteCount - g_VamAudioRenderBufferSize;
        g_VamAudioOverflowBytes += ByteCount - g_VamAudioRenderBufferSize;
        ByteCount = g_VamAudioRenderBufferSize;
    }

    KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);

    freeBytes = g_VamAudioRenderBufferSize - g_VamAudioAvailableBytes;
    if (ByteCount > freeBytes)
    {
        VamAudioTransportDropOldest(ByteCount - freeBytes);
    }

    firstCopyBytes = min(ByteCount, g_VamAudioRenderBufferSize - g_VamAudioWriteOffset);
    RtlCopyMemory(g_VamAudioRenderBuffer + g_VamAudioWriteOffset, Buffer, firstCopyBytes);

    remainingBytes = ByteCount - firstCopyBytes;
    if (remainingBytes > 0)
    {
        RtlCopyMemory(g_VamAudioRenderBuffer, Buffer + firstCopyBytes, remainingBytes);
    }

    g_VamAudioWriteOffset = (g_VamAudioWriteOffset + ByteCount) % g_VamAudioRenderBufferSize;
    g_VamAudioAvailableBytes += ByteCount;
    g_VamAudioTotalBytesWritten += ByteCount;

    KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);
}

_Use_decl_annotations_
NTSTATUS
VamAudioTransportDispatchCreateClose
(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp
)
{
    if (DeviceObject != g_VamAudioTransportDevice)
    {
        return PcDispatchIrp(DeviceObject, Irp);
    }

    VamAudioTransportCompleteIrp(Irp, STATUS_SUCCESS, 0);
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS
VamAudioTransportDispatchDeviceControl
(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp
)
{
    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;
    ULONG_PTR information = 0;
    PIO_STACK_LOCATION stack;
    PVOID systemBuffer;

    if (DeviceObject != g_VamAudioTransportDevice)
    {
        return PcDispatchIrp(DeviceObject, Irp);
    }

    stack = IoGetCurrentIrpStackLocation(Irp);
    systemBuffer = Irp->AssociatedIrp.SystemBuffer;

    switch (stack->Parameters.DeviceIoControl.IoControlCode)
    {
    case IOCTL_VAMAUDIO_READ_RENDER:
    {
        ULONG outputLength = stack->Parameters.DeviceIoControl.OutputBufferLength;
        ULONG bytesToRead;
        ULONG firstCopyBytes;
        KIRQL oldIrql;

        if (systemBuffer == NULL || outputLength == 0)
        {
            status = STATUS_SUCCESS;
            break;
        }

        KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);

        bytesToRead = min(outputLength, g_VamAudioAvailableBytes);
        firstCopyBytes = min(bytesToRead, g_VamAudioRenderBufferSize - g_VamAudioReadOffset);

        if (firstCopyBytes > 0)
        {
            RtlCopyMemory(systemBuffer, g_VamAudioRenderBuffer + g_VamAudioReadOffset, firstCopyBytes);
        }

        if (bytesToRead > firstCopyBytes)
        {
            RtlCopyMemory((PBYTE)systemBuffer + firstCopyBytes, g_VamAudioRenderBuffer, bytesToRead - firstCopyBytes);
        }

        g_VamAudioReadOffset = (g_VamAudioReadOffset + bytesToRead) % g_VamAudioRenderBufferSize;
        g_VamAudioAvailableBytes -= bytesToRead;
        g_VamAudioTotalBytesRead += bytesToRead;

        KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);

        status = STATUS_SUCCESS;
        information = bytesToRead;
        break;
    }

    case IOCTL_VAMAUDIO_GET_STATUS:
    {
        VAM_AUDIO_TRANSPORT_STATUS transportStatus;
        KIRQL oldIrql;
        ULONG captureActiveReaders = 0;
        ULONG captureMaxReaderAvailableBytes = 0;

        if (systemBuffer == NULL ||
            stack->Parameters.DeviceIoControl.OutputBufferLength < sizeof(VAM_AUDIO_TRANSPORT_STATUS))
        {
            status = STATUS_BUFFER_TOO_SMALL;
            break;
        }

        KeAcquireSpinLock(&g_VamAudioTransportLock, &oldIrql);

        transportStatus.BufferSize = g_VamAudioRenderBufferSize;
        transportStatus.AvailableBytes = g_VamAudioAvailableBytes;
        transportStatus.TotalBytesWritten = g_VamAudioTotalBytesWritten;
        transportStatus.TotalBytesRead = g_VamAudioTotalBytesRead;
        transportStatus.OverflowBytes = g_VamAudioOverflowBytes;
        transportStatus.SampleRate = g_VamAudioSampleRate;
        transportStatus.Channels = g_VamAudioChannels;
        transportStatus.BitsPerSample = g_VamAudioBitsPerSample;
        transportStatus.BlockAlign = g_VamAudioBlockAlign;
        transportStatus.CaptureBufferSize = g_VamAudioCaptureBufferSize;
        transportStatus.CaptureAvailableBytes = g_VamAudioCaptureAvailableBytes;
        transportStatus.CaptureTotalBytesWritten = g_VamAudioCaptureTotalBytesWritten;
        transportStatus.CaptureTotalBytesRead = g_VamAudioCaptureTotalBytesRead;
        transportStatus.CaptureOverflowBytes = g_VamAudioCaptureOverflowBytes;
        transportStatus.CaptureUnderrunBytes = g_VamAudioCaptureUnderrunBytes;
        for (ULONG index = 0; index < VAMAUDIO_CAPTURE_MAX_READERS; index++)
        {
            PVAM_AUDIO_CAPTURE_READER reader = &g_VamAudioCaptureReaders[index];
            if (reader->Active)
            {
                captureActiveReaders++;
                captureMaxReaderAvailableBytes = max(captureMaxReaderAvailableBytes, reader->AvailableBytes);
            }
        }
        transportStatus.CaptureActiveReaders = captureActiveReaders;
        transportStatus.CaptureMaxReaderAvailableBytes = captureMaxReaderAvailableBytes;

        KeReleaseSpinLock(&g_VamAudioTransportLock, oldIrql);

        RtlCopyMemory(systemBuffer, &transportStatus, sizeof(transportStatus));
        status = STATUS_SUCCESS;
        information = sizeof(transportStatus);
        break;
    }

    case IOCTL_VAMAUDIO_WRITE_CAPTURE:
    {
        ULONG inputLength = stack->Parameters.DeviceIoControl.InputBufferLength;

        if (systemBuffer == NULL || inputLength == 0)
        {
            status = STATUS_SUCCESS;
            break;
        }

        VamAudioTransportWriteCapture((PBYTE)systemBuffer, inputLength);

        status = STATUS_SUCCESS;
        information = inputLength;
        break;
    }

    default:
        break;
    }

    VamAudioTransportCompleteIrp(Irp, status, information);
    return status;
}
