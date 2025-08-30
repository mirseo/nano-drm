# mirseo_updrm

`mirseo_updrm`은 고성능 Rust 코어를 기반으로 하는 Python 라이브러리로, PNG 및 PDF 파일 내에 데이터를 안전하게 삽입하도록 설계되었습니다. 이 라이브러리는 오픈소스 디지털 저작권 관리(DRM) 또는 스테가노그래피 도구처럼 작동하여, JSON 메타데이터나 다른 이미지 파일과 같은 가변 크기의 정보를 호스트 파일에 첨부할 수 있게 해줍니다.

특히 리드-솔로몬(Reed-Solomon) 오류 정정 코드를 기반으로 하여, 호스트 파일이 부분적으로 손상되거나 잘려나가도 내부에 삽입된 데이터를 성공적으로 복구할 수 있습니다.

## 주요 기능

- **두 가지 파일 형식 지원:** PNG와 PDF 파일 모두에 데이터를 삽입할 수 있습니다.
- **자동 파일 탐지:** 불안정한 파일 확장자 대신, 파일의 실제 내용(매직 바이트)을 분석하여 타입을 결정합니다.
- **강력한 데이터 복구:** 리드-솔로mon 오류 정정 코드를 사용하여 손상된 파일에서도 데이터를 복구할 수 있습니다.
- **유연한 데이터 탑재:** JSON 문자열이나 다른 바이너리 파일 등, 바이트로 표현할 수 있는 모든 데이터를 삽입할 수 있습니다.
- **간단한 API:** `write`와 `read`라는 직관적인 인터페이스를 제공하여 쉽게 사용할 수 있습니다.
- **고성능:** 핵심 로직을 Rust로 구현하여 빠른 속도와 메모리 안전성을 보장합니다.

## 설치 방법

라이브러리를 설치하려면, 프로젝트의 루트 디렉토리로 이동한 후 `pip`을 사용하여 현재 환경에 설치합니다. 가상 환경을 사용하는 것을 강력히 권장합니다.

```bash
# 프로젝트 루트 디렉토리(pyproject.toml 파일이 있는 곳)에서 실행하세요.
# 수정 가능 모드(editable mode)로 패키지를 설치합니다.
pip install -e .
```

## 빠른 시작

API는 최대한 간단하게 사용할 수 있도록 설계되었습니다. 아래는 `write`와 `read` 메서드를 사용하는 예제입니다.

```python
import mirseo_updrm as mu
import json

# --- 설정 ---
# 대상 파일 경로 (PNG 또는 PDF)
file_path = "path/to/your/document.pdf"

# 삽입할 데이터
original_data = {
    "document_id": "doc-abc-123",
    "author": "Mirseo",
    "permissions": "read-only",
    "timestamp": 1678886400
}
# 딕셔너리를 JSON 문자열로 변환
json_string = json.dumps(original_data)


# --- 데이터 쓰기 ---
try:
    # write 함수는 원본 파일을 직접 수정하여 덧씌웁니다.
    mu.write(file_path, json_string)
    print(f"'{file_path}'에 데이터를 성공적으로 썼습니다.")
except Exception as e:
    print(f"쓰기 작업 중 오류 발생: {e}")


# --- 데이터 읽기 ---
try:
    # read 함수는 삽입된 원본 데이터를 바이트(bytes) 형태로 반환합니다.
    extracted_bytes = mu.read(file_path)
    
    # 바이트를 다시 문자열로 디코딩해야 합니다.
    extracted_string = extracted_bytes.decode('utf-8')
    
    # JSON 문자열을 다시 딕셔너리로 파싱합니다.
    retrieved_data = json.loads(extracted_string)
    
    print(f"읽어온 데이터: {retrieved_data}")
    
    # 원본과 일치하는지 검증
    assert original_data == retrieved_data
    print("데이터 검증 성공!")

except Exception as e:
    print(f"읽기 작업 중 오류 발생: {e}")

```

## API 레퍼런스

### `write(file_path: str, data: str | bytes)`

`file_path`로 지정된 파일에 데이터를 삽입하고, 원본 파일을 덧씌웁니다.

- **`file_path`**: 대상 PNG 또는 PDF 파일의 절대 또는 상대 경로입니다.
- **`data`**: 삽입할 데이터입니다. 직렬화된 JSON과 같은 UTF-8 문자열이거나, 다른 이미지 파일의 데이터와 같은 순수 바이트(bytes)일 수 있습니다.

### `read(file_path: str) -> bytes`

`file_path`로 지정된 파일에서 숨겨진 데이터를 추출합니다.

- **`file_path`**: 데이터가 삽입된 파일의 절대 또는 상대 경로입니다.
- **반환값**: 원본 데이터를 `bytes` 객체로 반환합니다. 원본 데이터가 문자열이었다면, `.decode('utf-8')`와 같이 디코딩이 필요합니다.

## 작동 원리

- **PNG 파일의 경우:** LSB(최하위 비트) 스테가노그래피 기법을 사용합니다. 이미지 픽셀을 구성하는 각 색상 채널(R, G, B, A)의 마지막 비트를 변경하여 데이터를 숨깁니다. 이 변화는 사람의 눈으로는 거의 감지할 수 없습니다.
- **PDF 파일의 경우:** 삽입할 데이터를 기반으로 작은 흑백 노이즈 이미지를 생성합니다. 그 후, 이 이미지를 PDF의 모든 페이지에 워터마크처럼 투명하게 오버레이하여 추가합니다.

## 소스 코드로 빌드하기

이 프로젝트는 `maturin`으로 빌드되었습니다. 개발을 위해 소스 코드로부터 직접 빌드하려면, 먼저 파이썬 가상 환경을 설정하세요. 그 후 프로젝트 루트 디렉토리에서 다음 명령을 실행합니다.

```bash
# 이 명령은 Rust 코드를 컴파일하고, 현재 가상 환경에
# 파이썬 패키지를 설치합니다.
maturin develop
```