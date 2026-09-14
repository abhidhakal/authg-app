import jsQR from 'jsqr';

export async function scanQrFromDataUrl(dataUrl: string): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      if (!ctx) {
        resolve(null);
        return;
      }

      canvas.width = img.width;
      canvas.height = img.height;
      ctx.drawImage(img, 0, 0, img.width, img.height);

      const imageData = ctx.getImageData(0, 0, img.width, img.height);
      let code = jsQR(imageData.data, imageData.width, imageData.height, {
        inversionAttempts: 'dontInvert',
      });

      if (!code) {
        code = jsQR(imageData.data, imageData.width, imageData.height, {
          inversionAttempts: 'attemptBoth',
        });
      }

      resolve(code ? code.data : null);
    };
    img.onerror = () => reject(new Error('Failed to load image'));
    img.src = dataUrl;
  });
}

export async function scanQrFromFile(file: File): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      if (typeof e.target?.result === 'string') {
        scanQrFromDataUrl(e.target.result).then(resolve).catch(reject);
      } else {
        resolve(null);
      }
    };
    reader.onerror = () => reject(new Error('Failed to read file'));
    reader.readAsDataURL(file);
  });
}

let sharedVideoCanvas: HTMLCanvasElement | null = null;

export function scanQrFromVideo(video: HTMLVideoElement): string | null {
  if (video.readyState < 2 || video.videoWidth === 0 || video.videoHeight === 0) {
    return null;
  }

  if (!sharedVideoCanvas) {
    sharedVideoCanvas = document.createElement('canvas');
  }

  if (sharedVideoCanvas.width !== video.videoWidth || sharedVideoCanvas.height !== video.videoHeight) {
    sharedVideoCanvas.width = video.videoWidth;
    sharedVideoCanvas.height = video.videoHeight;
  }

  const ctx = sharedVideoCanvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return null;

  ctx.drawImage(video, 0, 0, sharedVideoCanvas.width, sharedVideoCanvas.height);
  const imageData = ctx.getImageData(0, 0, sharedVideoCanvas.width, sharedVideoCanvas.height);
  const code = jsQR(imageData.data, imageData.width, imageData.height, {
    inversionAttempts: 'attemptBoth',
  });

  return code ? code.data : null;
}

