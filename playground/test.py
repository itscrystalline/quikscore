from turtle import width
from typing import Union
import cv2
from cv2.typing import MatLike


def triangle_crop(img: MatLike) -> MatLike:
    gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
    blur = cv2.GaussianBlur(gray, (5, 5), 0)
    thresh = cv2.adaptiveThreshold(
        blur, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY_INV, 11, 2
    )

    # Find contours
    (contours, _) = cv2.findContours(thresh, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
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
    img = img[xy1[1] : xy2[1], xy1[0] : xy2[0]]
    img = cv2.resize(img, None, fx=0.3333, fy=0.3333)
    return img


def split_into_areas(img: MatLike) -> list[MatLike]:
    splitted: list[MatLike] = []

    def split_percent(
        mat: MatLike, px: tuple[float, float], py: tuple[float, float]
    ) -> MatLike:
        shape: tuple[int, int, int] = mat.shape
        width, height, _ = shape
        start_x: int = width * px.index(0)
        start_y: int = height * py.index(0)
        end_x: int = width * px.index(1)
        end_y: int = height * py.index(1)
        return mat[start_x:end_x, start_y:end_y]

    # width = 1139 height = 791
    # subject name box
    splitted.append(split_percent(img, (0.01317, 0.1765), (0.1479, 0.1656)))
    # subject id box
    splitted.append(split_percent(img, (0, 0.0421), (0.2414, 0.5233)))
    # student name box
    splitted.append(split_percent(img, (0.0342, 0.1773), (0.1113, 0.1340)))
    # student id box
    splitted.append(split_percent(img, (0.04741, 0.2579), (0.1668, 0.5221)))

    return splitted


test_img = cv2.imread("../answer sheet/scan1/scan1_001.jpg")
grabbed = triangle_crop(test_img)
_ = cv2.imwrite("grabbed.png", grabbed)
for idx, img in enumerate(split_into_areas(grabbed)):
    _ = cv2.imwrite(f"area{idx}.png", grabbed)
