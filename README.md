# nano-drm

Reed-Solomon 오류 정정을 사용하여 PDF와 PNG 파일에 숨겨진 데이터를 삽입하고 추출할 수 있는 디지털 워터마킹 라이브러리입니다!

## 주요 기능

- **다중 포맷 지원**: PNG 이미지와 PDF 문서에 워터마크 삽입 가능
- **오류 정정**: Reed-Solomon 인코딩으로 파일 손상 시에도 데이터 복구 보장
- **스테가노그래피 삽입**: PNG는 LSB 스테가노그래피, PDF는 투명 객체를 사용한 비가시적 워터마크
- **Python 통합**: PyO3를 사용하여 Python 확장 모듈로 개발
- **데이터 무결성**: 추출 시 삽입된 데이터의 자동 검증

## 기술 개요

라이브러리 지원 워터마킹 기술은 다음과 같이 2개 서비스를 제공합니다!

### PNG 워터마킹
- LSB(Least Significant Bit) 스테가노그래피 사용
- RGBA 색상 채널에 데이터 삽입
- 데이터 무결성을 유지하면서 시각적 품질 보존
- 충분한 삽입 공간 확보를 위한 자동 용량 검사

### PDF 워터마킹  
- 워터마크 데이터를 포함하는 투명한 그레이스케일 이미지 생성
- PDF 페이지의 XObject 리소스로 이미지 삽입
- 비가시성을 보장하기 위해 최소 불투명도(1%) 사용
- 기존 PDF 구조 및 뷰어와 호환

### 오류 정정
- 10개 데이터 샤드와 4개 패리티 샤드를 사용한 Reed-Solomon 인코딩
- 부분적 데이터 손상으로부터 복구 가능
- 추출 시 손상된 워터마크 자동 재구성

## 설치

### 요구 사항
- Python 3.8 이상
- Rust 툴체인
- Maturin 빌드 도구  

추후 pypl로 제공 예정입니다!

### 소스에서 빌드

```bash
# 저장소 복제
git clone <repository-url>
cd nano-drm

# maturin 설치
pip install maturin

# 패키지 빌드 및 설치
maturin develop
```

## 사용법

### 기본 예제

```python
import nano_drm as drm
import json

# 삽입할 데이터 준비
data = {
    "user": "example_user",
    "project": "watermarking_test",
    "version": 1.0,
    "verified": True
}
json_data = json.dumps(data)

# 파일에 워터마크 삽입
drm.write("document.pdf", json_data)

# 파일에서 워터마크 추출
extracted_bytes = drm.read("document.pdf")
extracted_data = json.loads(extracted_bytes.decode('utf-8'))

print(f"추출된 데이터: {extracted_data}")
```

### 지원되는 파일 형식

```python
# PNG 이미지
drm.write("image.png", "숨겨진 메시지")

# PDF 문서
drm.write("document.pdf", json.dumps({"metadata": "value"}))

# 바이너리 데이터 삽입
binary_data = b"\x01\x02\x03\x04"
drm.write("file.png", binary_data)
```

## API 참조

### `write(file_path: str, data: Union[str, bytes]) -> None`

지정된 파일에 데이터를 삽입합니다.

**매개변수:**
- `file_path`: 대상 PNG 또는 PDF 파일 경로
- `data`: 삽입할 데이터 (문자열 또는 바이트)

**예외:**
- `IOError`: 파일 접근 오류
- `ValueError`: 지원되지 않는 파일 형식 또는 용량 부족
- `TypeError`: 유효하지 않은 데이터 타입

### `read(file_path: str) -> bytes`

지정된 파일에서 삽입된 데이터를 추출합니다.

**매개변수:**
- `file_path`: 워터마크가 삽입된 파일 경로

**반환값:**
- `bytes`: 추출된 데이터

**예외:**
- `IOError`: 파일 접근 오류
- `ValueError`: 워터마크를 찾을 수 없거나 복구 불가능한 손상

## 오류 처리

라이브러리는 다양한 시나리오에 대한 포괄적인 오류 처리를 제공합니다:

```python
try:
    drm.write("document.pdf", large_data)
except ValueError as e:
    if "Not enough space" in str(e):
        print("데이터에 비해 파일 크기가 너무 작습니다")
    elif "Unsupported file type" in str(e):
        print("지원되지 않는 파일 형식입니다")
        
try:
    data = drm.read("watermarked.png")
except ValueError as e:
    if "No embedded data found" in str(e):
        print("파일에 워터마크가 없습니다")
    elif "reconstruction" in str(e):
        print("워터마크가 복구 불가능할 정도로 손상되었습니다")
```

## 테스트

프로젝트에서 제공하는 공식 테스트입니다!

```bash
# PNG 테스트 실행
python test/test_run.py

# PDF 테스트 실행
python test/test_pdf.py

# 커스텀 테스트 실행
python test/test_custom_code.py
```

## 기술적 세부사항

### Reed-Solomon 설정
- 데이터 샤드: 10개
- 패리티 샤드: 4개
- 전체 중복도: 40%
- 최대 복구 가능한 손상: 14개 중 4개 샤드

### PNG 삽입 사양
- 방법: RGBA 채널의 LSB 스테가노그래피
- 용량: 색상 채널당 1비트 (RGBA의 경우 픽셀당 4비트)
- 데이터 형식: 길이 헤더(8바이트) + Reed-Solomon 인코딩된 페이로드

### PDF 삽입 사양
- 방법: 투명한 그레이스케일 XObject 이미지
- 불투명도: 1% (ca 값 0.01)
- 위치: 고정된 50x50 오프셋과 10x10 스케일링
- 객체 명명: 이미지는 "UpdrmImg", 그래픽 상태는 "UpdrmGS"

## 라이선스

이 프로젝트는 MIT 라이선스 하에 제공됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참조하세요.

## 기여하기

1. 저장소를 포크합니다
2. 기능 브랜치를 생성합니다
3. 변경사항을 적용합니다
4. 새로운 기능에 대한 테스트를 추가합니다
5. 모든 테스트가 통과하는지 확인합니다
6. 풀 리퀘스트를 제출합니다

## 제한사항

- PNG 파일은 데이터 페이로드에 충분한 픽셀 용량을 가져야 합니다
- PDF 삽입은 PDF 구조에 가시적 객체를 생성합니다 (시각적으로는 투명함)
- Reed-Solomon 인코딩은 페이로드 크기에 약 40%의 오버헤드를 추가합니다
- 바이너리 데이터는 지원되지만 사용 가능한 삽입 용량 내에 맞아야 합니다