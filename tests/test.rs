#[cfg(test)]
mod test {
    use std::{
        task::Poll,
        time::{Duration, Instant},
    };

    use tokio::time::sleep;

    struct PrintFuture {
        end_time: Option<Instant>,
    }
    impl PrintFuture {
        fn new() -> Self {
            Self {
                end_time: Instant::now().checked_add(Duration::from_secs(5)),
            }
        }
    }
    impl Future for PrintFuture {
        type Output = ();

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            if let Some(end_time) = &self.end_time {
                let now = Instant::now().duration_since(*end_time);
                if now.is_zero() {
                    println!("future poll---wait");
                    let waker = cx.waker().clone();
                    tokio::spawn(async move {
                        sleep(Duration::from_secs(1)).await;
                        waker.wake_by_ref();
                    });

                    return Poll::Pending;
                } else {
                    println!("future finished success!!!");
                    return Poll::Ready(());
                }
            } else {
                println!("no end time , print this msg");
                return Poll::Ready(());
            }
        }
    }
    #[test]
    fn test_future() {
        if let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            rt.block_on(async {
                PrintFuture::new().await;
            });
        }
    }
    fn compute_medium(arr: &[i32], i: &usize, j: &usize, k: &usize) -> usize {
        let a = arr[*i];
        let b = arr[*j];
        let c = arr[*k];
        if a > b {
            if b > c {
                *j
            } else if a > c {
                *k
            } else {
                *i
            }
        } else if a > c {
            *i
        } else if b > c {
            *k
        } else {
            *j
        }
    }
    // 手写采用中值pivot,三路分区和双向扫描的快速排序
    // 快速排序里的双索引扫描双索引并不是第一移动元素，
    // 存在第三个作为第一移动的索引
    fn quick_sort(arr: &mut [i32]) {
        if arr.len() < 2 {
            return;
        }
        let first = 0;
        let mid = arr.len() / 2;
        let last = arr.len() - 1;
        let pivot = compute_medium(arr, &first, &mid, &last);
        println!("pivot{:?}", arr[pivot]);
        arr.swap(last, pivot);
        let (mut left_index, mut idx, mut right_index) = (0, 0, last - 1);
        loop {
            if idx > right_index {
                break;
            }
            if arr[idx] > arr[last] {
                arr.swap(idx, right_index);
                if right_index == 0 {
                    break;
                }
                right_index -= 1;
            } else if arr[idx] < arr[last] {
                arr.swap(idx, left_index);
                left_index += 1;
                if idx < left_index {
                    idx += 1;
                }
            } else {
                idx += 1;
            }
        }
        arr.swap(idx, last);

        let (lt, egt) = arr.split_at_mut(left_index);

        let (e, gt) = egt.split_at_mut(idx + 1 - lt.len());
        println!("eq{:?}", e);
        quick_sort(lt);
        quick_sort(gt);
    }
    pub fn shell_sort<T: Ord>(arr: &mut [T]) {
        let n = arr.len();

        // 生成 Tokuda 序列
        let mut gaps = Vec::new();
        let mut k = 1;
        loop {
            let gap = ((9_i64.pow(k) - 4_i64.pow(k)) / (5 * 4_i64.pow(k - 1))) as usize;
            if gap > n {
                break;
            }
            gaps.push(gap);
            k += 1;
        }
        gaps.reverse(); // 从大到小使用

        // 希尔排序核心
        for &gap in &gaps {
            for i in gap..n {
                let mut j = i;

                loop {
                    if j >= n {
                        break;
                    }
                    if arr[j - gap] > arr[j] {
                        arr.swap(j - gap, j);
                    }
                    j += 1;
                }
            }
        }
    }
    #[test]
    fn test_quick_sort() {
        let mut arr = [
            1236985745, 213, 213, -3069, 2000, 213, 569, 265, 231, 444, 578, 032, -136, 123, 589,
            987, 625, 301, 203, 10, 9999,
        ];
        quick_sort(&mut arr);
        println!("{:?}", arr);
    }
    #[test]
    fn test_shell_sort() {
        let mut arr = [
            1236985745, 213, 213, -3069, 2000, 213, 569, 265, 231, 444, 578, 032, -136, 123, 589,
            987, 625, 301, 203, 10, 9999,
        ];
        shell_sort(&mut arr);
        println!("{:?}", arr);
    }
    #[derive(Debug)]
    enum DivisionError {
        // Example: 42 / 0
        DivideByZero,
        // Only case for `i64`: `i64::MIN / -1` because the result is `i64::MAX + 1`
        IntegerOverflow,
        // Example: 5 / 2 = 2.5
        NotDivisible,
    }
    fn divide(a: i64, b: i64) -> Result<i64, DivisionError> {
        if b == 0 {
            return Err(DivisionError::DivideByZero);
        }

        if a == i64::MIN && b == -1 {
            return Err(DivisionError::IntegerOverflow);
        }

        if a % b != 0 {
            return Err(DivisionError::NotDivisible);
        }

        Ok(a / b)
    }
    #[test]
    fn testdivide() {
        let divide = divide(20, 0).unwrap();
        println!("{:#?}", divide);
        println!("test success");
    }
    #[test]
    fn dian_bing() {
        let num = 3 * 5 * 8;
        let mut n1 = 0;
        let mut temp = 5 * 8;
        let mut mul_times = 1;
        loop {
            if n1 % 3 == 1 {
                break;
            }
            n1 = temp * mul_times;
            mul_times += 1;
        }
        let mut n2 = 0;
        temp = 3 * 8;
        mul_times = 1;
        loop {
            if n2 % 5 == 1 {
                break;
            }
            n2 = temp * mul_times;
            mul_times += 1;
        }
        let mut n3 = 0;
        temp = 3 * 5;
        mul_times = 1;
        loop {
            if n3 % 8 == 1 {
                break;
            }
            n3 = temp * mul_times;
            mul_times += 1;
        }
        println!("n1:{},n2:{},n3:{}", n1, n2, n3);
        let mut ans = n1 * 2 + n2 * 3 + n3 * 5;
        loop {
            if ans >= 200 && ans <= 400 {
                break;
            } else if ans < 200 {
                ans += num;
            } else {
                ans -= num;
            }
        }
        println!("answer:{}", ans);
    }
    #[test]
    fn test_sub_set_sum() {
        // 3, 34, 4, 12, 5, 2 sum 9
        // 1, 2, 3 sum 6
        // 5 ,10 ,12 ,13 ,15 ,18  sum 30
        // 4,8,5 sum 9
        // 267, 493, 869, 112, 367, 984, 145, 723, 555, 802, 212, 996, 703, 810, 412 sum 3680
        // 102, 205, 307, 403, 502, 608, 701, 809, 904, 1006, 1103, 1205, 1307, 1403, 1502 s 5001 false
        // 100, 99, 98, 97, 96, 95, 20, 19, 18, 17, 16, 15, 5, 4, 3, 2, 1 sum 350
        // 200, 200, 200, 200, 200, 1, 1, 1, 1, 1 sum 700 false
        // let mut arr = vec![200, 200, 200, 200, 200, 1, 1, 1, 1, 1];
        // let sum = 700;
        // arr.sort();
        // arr.reverse();
        // let mut ans_arr = vec![];
        // println!(
        //     "result:{},chose elements{:?}",
        //     is_sub_set_sum_recursion(&arr, &sum, &mut ans_arr, arr.iter().sum::<i32>(), 0, 0),
        //     ans_arr
        // );
        // println!("result:{}", is_sub_set_sum(&mut arr, &sum));
        // println!("result:{}", is_sub_set_sum_dp(&arr, &sum));
    }
    fn _is_sub_set_sum_recursion(
        arr: &[i32],
        sum: &i32,
        ans_arr: &mut Vec<i32>,
        remain_elements_sum: i32,
        temp_sum: i32,
        index: usize,
    ) -> bool {
        if temp_sum > *sum {
            return false;
        }
        if temp_sum == *sum {
            return true;
        }
        if index >= arr.len() {
            return false;
        }
        let upper_bound = temp_sum + remain_elements_sum;
        if upper_bound < *sum {
            return false;
        }
        ans_arr.push(arr[index]);
        let result_chose = _is_sub_set_sum_recursion(
            arr,
            sum,
            ans_arr,
            remain_elements_sum - arr[index],
            temp_sum + arr[index],
            index + 1,
        );
        if !result_chose {
            ans_arr.remove(ans_arr.len() - 1);
            let result_skip = _is_sub_set_sum_recursion(
                arr,
                sum,
                ans_arr,
                remain_elements_sum - arr[index],
                temp_sum,
                index + 1,
            );
            return result_skip;
        }
        result_chose
    }
    fn _is_sub_set_sum(arr: &mut [i32], sum: &i32) -> bool {
        arr.sort();
        arr.reverse();
        let mut remain_elements_sum = arr.iter().sum::<i32>();
        let mut condition_arr = vec![0_u8; arr.len()];
        let mut index = 0_i32;
        let mut pre_value = 0;
        let mut ans_arr = vec![];
        let mut searched_times = 0;
        loop {
            if index < 0 {
                break;
            }
            if pre_value == *sum {
                println!("chose elements{:?}", ans_arr);

                return true;
            }
            if index as usize == arr.len() || condition_arr[index as usize] == 2 {
                if (index as usize) < arr.len() {
                    condition_arr[index as usize] = 0;
                    remain_elements_sum = remain_elements_sum + arr[index as usize];
                }
                index -= 1;
            } else if condition_arr[index as usize] == 1 {
                condition_arr[index as usize] = 2;
                ans_arr.remove(ans_arr.len() - 1);
                pre_value = pre_value - arr[index as usize];
                index += 1;
            } else {
                searched_times += 1;
                // 第一次到达下标 尝试添加
                if pre_value > *sum {
                    condition_arr[index as usize] = 2;
                    continue;
                }
                remain_elements_sum = remain_elements_sum - arr[index as usize];
                let upper_bound = pre_value + remain_elements_sum;
                if upper_bound < *sum {
                    // println!("searched times{}", searched_times);
                    // break;
                }
                ans_arr.push(arr[index as usize]);
                pre_value = pre_value + arr[index as usize];

                condition_arr[index as usize] = 1;
                index += 1;
            }
        }
        println!("searched times{}", searched_times);
        false
    }
    // fn is_sub_set_sum_dp(arr: &[i32], sum: &i32) -> bool {
    //     let mut is_eq_temp_sum = vec![vec![false; (*sum + 1) as usize]; arr.len()];
    //     is_eq_temp_sum[0][arr[0] as usize] = true;
    //     for index in 0..is_eq_temp_sum.len() {
    //         is_eq_temp_sum[index][0] = true;
    //     }
    //     for index in 1..is_eq_temp_sum.len() {
    //         for temp_sum in 1..(*sum + 1) {
    //             let mut is_eq_sum_tmp = false;
    //             if temp_sum - arr[index] >= 0 {
    //                 is_eq_sum_tmp = is_eq_temp_sum[index - 1][(temp_sum - arr[index]) as usize];
    //             }
    //             is_eq_temp_sum[index][temp_sum as usize] =
    //                 is_eq_temp_sum[index - 1][temp_sum as usize] || is_eq_sum_tmp;
    //         }
    //     }
    //     is_eq_temp_sum[arr.len() - 1][*sum as usize]
    // }
}
