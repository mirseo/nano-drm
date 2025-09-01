import nano_drm as mu
import json
import os
import shutil

def run_pdf_test():
    # 1. Setup paths and data
    source_path = os.path.join('test', 'example.pdf')
    test_path = os.path.join('test', 'example_pdf_test_copy.pdf')
    
    original_data = {
        "user": "Mirseo_PDF_Test",
        "project": "UPDRM_PDF",
        "version": 1.0,
        "verified": True,
        "data": [1, 2, 3, 4, 5]
    }
    json_string_data = json.dumps(original_data)

    print(f"--- Starting PDF Test ---")
    print(f"Original data: {json_string_data}")

    # 2. Create a copy for testing
    shutil.copy(source_path, test_path)
    print(f"Created a temporary test file: {test_path}")

    try:
        # 3. Write data to the test PDF
        print("\nStep 1: Writing data with mu.write()...")
        mu.write(test_path, json_string_data)
        print("Write operation completed.")

        # 4. Read data from the test PDF
        print("\nStep 2: Reading data with mu.read()...")
        extracted_bytes = mu.read(test_path)
        print("Read operation completed.")

        # 5. Verify the data
        print("\nStep 3: Verifying data...")
        if not extracted_bytes:
            print("FAILURE: No data was extracted.")
            return

        extracted_string = extracted_bytes.decode('utf-8')
        extracted_data = json.loads(extracted_string)
        print(f"Extracted data: {extracted_data}")

        if extracted_data == original_data:
            print("\n--- SUCCESS: PDF Extracted data matches original data. ---")
        else:
            print("\n--- FAILURE: PDF Extracted data does NOT match original data. ---")

    except Exception as e:
        print(f"\n--- An error occurred during the PDF test: {e} ---")
    finally:
        # 6. Clean up the test file
        if os.path.exists(test_path):
            os.remove(test_path)
            print(f"\nCleaned up temporary file: {test_path}")

if __name__ == "__main__":
    run_pdf_test()
