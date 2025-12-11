#include <algorithm> // for std::max
#include <iostream>
#include <vector>

// 求最大子数组和（Kadane算法）
int maxSubArraySum(const std::vector<int> &arr) {
  int maxSum = arr[0];     // 记录全局最大子数组和
  int currentSum = arr[0]; // 当前子数组和

  for (size_t i = 1; i < arr.size(); ++i) {
    // 如果加上 arr[i] 反而更小，就从 arr[i] 重新开始
    currentSum = std::max(arr[i], currentSum + arr[i]);
    // 更新全局最大值
    maxSum = std::max(maxSum, currentSum);
  }

  return maxSum;
}

int main() {
  std::vector<int> arr = {-24, -99, 35, 5, 25, -10, 22, -99, 111};
  std::cout << "最大子数组和: " << maxSubArraySum(arr) << std::endl;
  return 0;
}