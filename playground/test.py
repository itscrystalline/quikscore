import cv2
from cv2.typing import MatLike, Scalar


def triangle_crop(img: MatLike) -> MatLike:
    img = split_percent(img, (0.00570288, 0.99714856), (0.008064516, 0.995967742))
    img = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
    blur = cv2.GaussianBlur(img, (5, 5), 0)
    thresh = cv2.adaptiveThreshold(
        blur, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY_INV, 11, 2
    )

    # Find contours
    (contours, _) = cv2.findContours(thresh, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    _ = cv2.imwrite("contours.png", thresh)
    xy1 = [10, 10]
    xy2 = [10, 10]

    for cnt in contours:
        # Approximate the contour

        epsilon = 0.04 * cv2.arcLength(cnt, True)
        length_approx = cv2.arcLength(cnt, True)
        approx = cv2.approxPolyDP(cnt, epsilon, True)

        # Check if it's a triangle
        if length_approx > 90:
            if len(approx) == 3:
                M = cv2.moments(cnt)
                cx = int(M["m10"] / M["m00"])
                cy = int(M["m01"] / M["m00"])

                if (cx + cy) < 300:
                    xy1 = [cx, cy]
                if cx > 1000:
                    xy2[0] = cx
                elif cy > 1000:
                    xy2[1] = cy

    # Display the result
    print(f"{xy1[1]}:{xy2[1]}, {xy1[0]}:{xy2[0]}")
    img = img[xy1[1] : xy2[1], xy1[0] : xy2[0]]
    img = cv2.resize(img, None, fx=0.3333, fy=0.3333)
    return img


START_PERCENT_X = 0.18525022
START_PERCENT_Y = 0.010113780
WIDTH_PERCENT = 0.19841967
HEIGHT_PERCENT = 0.094816688
GAP_X_PERCENT = 0.0079016681
GAP_Y_PERCENT = 0.015170670


def split_percent(
    mat: MatLike, px: tuple[float, float], py: tuple[float, float]
) -> MatLike:
    shape: tuple[int, int] = mat.shape
    height, width = shape[0], shape[1]
    print(f"width {width}, height {height}")
    start_x: int = int(width * px[0])
    start_y: int = int(height * py[0])
    end_x: int = int(width * px[1])
    end_y: int = int(height * py[1])
    print(f"{start_y}:{end_y}, {start_x}:{end_x}")
    return mat[start_y:end_y, start_x:end_x].copy()


def split_into_areas(img: MatLike) -> list[MatLike]:
    splitted: list[MatLike] = []

    def mark_percent(
        mat: MatLike, px: tuple[float, float], py: tuple[float, float], color: Scalar
    ) -> MatLike:
        shape: tuple[int, int] = mat.shape
        height, width = shape[0], shape[1]
        start_x: int = int(width * px[0])
        start_y: int = int(height * py[0])
        end_x: int = int(width * px[1])
        end_y: int = int(height * py[1])
        return cv2.rectangle(mat, (start_x, start_y), (end_x, end_y), color)

    # width = 1139 height = 791
    # subject name box
    splitted.append(split_percent(img, (0.01317, 0.1765), (0.1479, 0.1656)))
    # subject id box
    subject_id = split_percent(img, (0, 0.040386304), (0.271807838, 0.517067004))
    for i in range(3):
        subject_id = mark_percent(
            subject_id, (i / 3, (i + 1) / 3), (0.128205, 1), (0, 255, 0)
        )
        for j in range(10):
            subject_id = mark_percent(
                subject_id, (0, 1), (j / 9, (j + 1) / 9), (255, 0, 0)
            )
    splitted.append(subject_id)
    # student name box
    splitted.append(split_percent(img, (0.0342, 0.1773), (0.1113, 0.1340)))
    # student id box
    student_id = split_percent(
        img, (0.049165935, 0.177348551), (0.273072061, 0.515802781)
    )
    for i in range(9):
        student_id = mark_percent(
            student_id, (i / 9, (i + 1) / 9), (0.12565445, 1), (0, 255, 0)
        )
        for j in range(10):
            student_id = mark_percent(
                student_id, (0, 1), (j / 9, (j + 1) / 9), (255, 0, 0)
            )
    splitted.append(student_id)
    # exam room
    splitted.append(
        split_percent(img, (0.032484636, 0.088674276), (0.206068268, 0.230088496))
    )
    # exam seat
    splitted.append(
        split_percent(img, (0.134328358, 0.175592625), (0.206068268, 0.230088496))
    )

    for x in range(0, 4):
        for y in range(0, 9):
            min_x = START_PERCENT_X + x * (GAP_X_PERCENT + WIDTH_PERCENT)
            max_x = min(min_x + WIDTH_PERCENT, 1.0)
            min_y = START_PERCENT_Y + y * (GAP_Y_PERCENT + HEIGHT_PERCENT)
            max_y = min(min_y + HEIGHT_PERCENT, 1.0)
            ans = split_percent(
                img,
                (min_x, max_x),
                (min_y, max_y),
            )
            for i in range(5):
                ans = mark_percent(
                    ans, (0.11946903, 1), (i / 5, (i + 1) / 5), (0, 255, 0)
                )
                left = 1 - 0.11946903
                for j in range(13):
                    ans = mark_percent(
                        ans,
                        (
                            0.11946903 + (j / 13) * left,
                            (0.11946903 + (j + 1) / 13) * left,
                        ),
                        (i / 5, (i + 1) / 5),
                        (255, 0, 0),
                    )
            splitted.append(ans)

    return splitted


# test_img = cv2.imread("../src-tauri/tests/assets/image_001.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/image_002.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/image_003.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/scan1_001.jpg")
test_img = cv2.imread("../src-tauri/tests/assets/scan1_002.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/scan1_003.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/scan2_001.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/scan2_002.jpg")
# test_img = cv2.imread("../src-tauri/tests/assets/scan2_003.jpg")
_ = cv2.imwrite("test.png", test_img)
grabbed = triangle_crop(test_img)
_ = cv2.imwrite("grabbed.png", grabbed)
_, grabbed = cv2.threshold(grabbed, 165, 255, cv2.THRESH_BINARY)
_ = cv2.imwrite("grabbed_thresh.png", grabbed)
for idx, img in enumerate(split_into_areas(grabbed)):
    _ = cv2.imwrite(f"area{idx}.png", img)
