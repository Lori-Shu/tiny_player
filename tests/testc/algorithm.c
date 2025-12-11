#include "memory.h"
#include "stdint.h"
#include "stdio.h"
#include "stdlib.h"
int32_t dian_bing() {

  int32_t num = 3 * 5 * 7;
  int32_t n1 = 0;
  int32_t temp = 5 * 7;
  int32_t mul_times = 1;
  for (;;) {
    if (n1 % 3 == 1) {
      break;
    }
    n1 = temp * mul_times;
    mul_times += 1;
  }
  int32_t n2 = 0;
  temp = 3 * 7;
  mul_times = 1;
  for (;;) {
    if (n2 % 5 == 1) {
      break;
    }
    n2 = temp * mul_times;
    mul_times += 1;
  }
  int32_t n3 = 0;
  temp = 3 * 5;
  mul_times = 1;
  for (;;) {
    if (n3 % 7 == 1) {
      break;
    }
    n3 = temp * mul_times;
    mul_times += 1;
  }
  printf("n1%d,n2%d,n3%d", n1, n2, n3);
  int32_t ans = n1 * 2 + n2 * 3 + n3 * 2;
  for (;;) {
    if (ans < num) {
      return ans;
    } else {
      ans -= num;
    }
  }
}

int32_t ret_max(int32_t a, int32_t b) {
  if (a > b) {
    return a;
  } else {
    return b;
  }
}
typedef struct MaxSubArrSumRes {
  int32_t max_from_left;
  int32_t max_from_right;
  int32_t max_mid_res;
  int32_t subarr_sum;
} MaxSubArrSumRes_t;

MaxSubArrSumRes_t max_subarr_sum(int32_t arr[], uint64_t left_index,
                                 uint64_t right_index) {

  if (left_index == right_index) {
    MaxSubArrSumRes_t res = {.max_from_left = arr[left_index],
                             .max_from_right = arr[left_index],
                             .max_mid_res = ret_max(arr[left_index], 0),
                             .subarr_sum = arr[left_index]};
    return res;
  }
  MaxSubArrSumRes_t res;
  MaxSubArrSumRes_t left_res =
      max_subarr_sum(arr, left_index, (left_index + right_index) / 2);
  MaxSubArrSumRes_t right_res =
      max_subarr_sum(arr, (left_index + right_index) / 2 + 1, right_index);
  res.max_from_left = ret_max(left_res.max_from_left,
                              left_res.subarr_sum + right_res.max_from_left);
  res.max_from_right = ret_max(right_res.max_from_right,
                               right_res.subarr_sum + left_res.max_from_right);
  res.max_mid_res =
      ret_max(left_res.max_from_right + right_res.max_from_left,
              ret_max(left_res.max_mid_res, right_res.max_mid_res));
  res.subarr_sum = left_res.subarr_sum + right_res.subarr_sum;
  return res;
}

int32_t max_subarr_sum_dp(int32_t arr[], uint64_t left_index,
                          uint64_t right_index) {

  int32_t length = right_index - left_index + 1;
  int32_t **max_on_start_to_end =
      (int32_t **)malloc(length * sizeof(int32_t *));
  int32_t idx = 0;
  for (;;) {
    if (idx > right_index) {
      break;
    }
    max_on_start_to_end[idx] = (int32_t *)malloc(length * sizeof(int32_t));
    memset(max_on_start_to_end[idx], 0, length * sizeof(int32_t));
    idx += 1;
  }
  int32_t *sum_from_start = (int32_t *)malloc(length * sizeof(int32_t));
  memset(sum_from_start, 0, length * sizeof(int32_t));
  int32_t dst_idx = left_index;
  for (;;) {
    if (dst_idx > right_index) {
      break;
    }
    if (dst_idx == 0) {
      sum_from_start[0] = arr[0];
    } else {
      sum_from_start[dst_idx] = sum_from_start[dst_idx - 1] + arr[dst_idx];
    }
    int32_t start_idx = dst_idx;
    for (;;) {
      if (start_idx < 0) {
        break;
      }
      if (dst_idx == start_idx) {
        // max_on_start_to_end[dst_idx][start_idx] = ret_max(arr[dst_idx], 0);
        max_on_start_to_end[start_idx][dst_idx] = arr[dst_idx];
      } else {
        int32_t sum_choose_all = 0;
        if (start_idx == 0) {
          sum_choose_all = sum_from_start[dst_idx];
        } else {
          sum_choose_all =
              sum_from_start[dst_idx] - sum_from_start[start_idx - 1];
        }

        max_on_start_to_end[start_idx][dst_idx] =
            ret_max(sum_choose_all,
                    ret_max(max_on_start_to_end[start_idx][dst_idx - 1],
                            max_on_start_to_end[start_idx + 1][dst_idx]));
      }

      start_idx -= 1;
    }
    dst_idx += 1;
  }
  int32_t ans = max_on_start_to_end[left_index][right_index];
  idx = 0;
  for (;;) {
    if (idx > right_index) {
      break;
    }
    free(max_on_start_to_end[idx]);
    idx += 1;
  }

  free(max_on_start_to_end);
  free(sum_from_start);
  return ans;
}
int32_t max_subarr_sum_dp2(int32_t arr[], uint64_t size) {
  int32_t *max_to = (int32_t *)malloc(size * sizeof(int32_t));
  memset(max_to, 0, size * sizeof(int32_t));
  int32_t index = 0;
  int32_t ans = arr[0];
  for (;;) {
    if (index >= size) {
      break;
    }
    if (index == 0) {
      max_to[index] = arr[index];
    }else{
      max_to[index] = ret_max(arr[index], max_to[index - 1] + arr[index]);
    }
    if (max_to[index]>ans) {
      ans = max_to[index];
    }
    index += 1;
  }
  if(ans<0){
    ans = 0;
  }
  free(max_to);
  return ans;
}

int main() {
  // 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
  //   -2,1,-3,4,-1,2,1,-5,4
  // -2,-1,-7,-3,-52
  //   -24,-99,35,5,25,-10,22,-99,111
  //   8,-19,5,-4,20
//   -3,-3,5,-2,-1,2,6,2
  int32_t arr[] = {-3, -3, 5, -2, -1, 2, 6, 2};

  int32_t ans = max_subarr_sum_dp2(arr, 8);
  printf("answer%d\n", ans);
}
