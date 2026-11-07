// 버튼 이미지 처리: 고른/붙여넣은 이미지를 적당한 크기의 PNG data URL 로 줄여서 설정에 저장한다.

/** 긴 변 기준 최대 크기 (버튼 전체를 채워도 선명하고, 설정 파일이 너무 커지지 않는 정도) */
const MAX_SIDE = 192;
/** 이보다 큰 원본은 거부 (실수로 수십 MB 사진을 넣는 경우) */
const MAX_FILE_BYTES = 20 * 1024 * 1024;

export async function imageToDataUrl(file: Blob): Promise<string> {
  if (!file.type.startsWith("image/")) throw new Error("이미지 파일이 아니에요");
  if (file.size > MAX_FILE_BYTES) throw new Error("이미지가 너무 커요 (20MB 이하)");

  const bitmap = await createImageBitmap(file).catch(() => {
    throw new Error("이미지를 읽지 못했어요");
  });
  const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
  const w = Math.max(1, Math.round(bitmap.width * scale));
  const h = Math.max(1, Math.round(bitmap.height * scale));

  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("이미지를 처리하지 못했어요");
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bitmap, 0, 0, w, h);
  bitmap.close();
  return canvas.toDataURL("image/png");
}

/** 붙여넣기/끌어놓기 데이터에서 첫 번째 이미지 파일 */
export function firstImage(data: DataTransfer | null): File | null {
  if (!data) return null;
  for (const f of Array.from(data.files)) if (f.type.startsWith("image/")) return f;
  for (const item of Array.from(data.items ?? [])) {
    if (item.kind === "file" && item.type.startsWith("image/")) return item.getAsFile();
  }
  return null;
}
