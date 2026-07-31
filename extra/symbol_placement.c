// Bu program specification'ın Ek F.1 bölümünden kopyalanmış ve test data
// üretmek için küçük değişiklikler yapılmıştır.
#include <stdio.h>
#include <stdlib.h>

int nrow, ncol, *array;

/* "module", "chr+bit" değerini array[] içinde uygun wrapping ile yerleştirir. */
void module(int row, int col, int chr, int bit) {
  if (row < 0) {
    row += nrow;
    col += 4 - ((nrow + 4) % 8);
  }
  if (col < 0) {
    col += ncol;
    row += 4 - ((ncol + 4) % 8);
  }
  // DMRE için
  if (row >= nrow) {
    row -= nrow;
  }
  array[row * ncol + col] = 10 * chr + bit;
}

/* "utah", Utah biçimli symbol karakterinin 8 bitini ECC200 içine yerleştirir. */
void utah(int row, int col, int chr) {
  module(row - 2, col - 2, chr, 1);
  module(row - 2, col - 1, chr, 2);
  module(row - 1, col - 2, chr, 3);
  module(row - 1, col - 1, chr, 4);
  module(row - 1, col, chr, 5);
  module(row, col - 2, chr, 6);
  module(row, col - 1, chr, 7);
  module(row, col, chr, 8);
}

/* "cornerN", dört özel köşe durumunun 8 bitini ECC200 içine yerleştirir. */
void corner1(int chr) {
  module(nrow - 1, 0, chr, 1);
  module(nrow - 1, 1, chr, 2);
  module(nrow - 1, 2, chr, 3);
  module(0, ncol - 2, chr, 4);
  module(0, ncol - 1, chr, 5);
  module(1, ncol - 1, chr, 6);
  module(2, ncol - 1, chr, 7);
  module(3, ncol - 1, chr, 8);
}

void corner2(int chr) {
  module(nrow - 3, 0, chr, 1);
  module(nrow - 2, 0, chr, 2);
  module(nrow - 1, 0, chr, 3);
  module(0, ncol - 4, chr, 4);
  module(0, ncol - 3, chr, 5);
  module(0, ncol - 2, chr, 6);
  module(0, ncol - 1, chr, 7);
  module(1, ncol - 1, chr, 8);
}

void corner3(int chr) {
  module(nrow - 3, 0, chr, 1);
  module(nrow - 2, 0, chr, 2);
  module(nrow - 1, 0, chr, 3);
  module(0, ncol - 2, chr, 4);
  module(0, ncol - 1, chr, 5);
  module(1, ncol - 1, chr, 6);
  module(2, ncol - 1, chr, 7);
  module(3, ncol - 1, chr, 8);
}

void corner4(int chr) {
  module(nrow - 1, 0, chr, 1);
  module(nrow - 1, ncol - 1, chr, 2);
  module(0, ncol - 3, chr, 3);
  module(0, ncol - 2, chr, 4);
  module(0, ncol - 1, chr, 5);
  module(1, ncol - 3, chr, 6);
  module(1, ncol - 2, chr, 7);
  module(1, ncol - 1, chr, 8);
}

/* "ECC200", nrow × ncol array'i ECC200 için uygun değerlerle doldurur. */
void ECC200(void) {
  int row, col, chr;

  /* Önce array[] öğesini geçersiz girdilerle doldurur. */
  for (row = 0; row < nrow; row++) {
    for (col = 0; col < ncol; col++) {
      array[row * ncol + col] = 0;
    }
  }
  /* 1. karakterin 8. biti için doğru konumdan başlar. */
  chr = 1;
  row = 4;
  col = 0;

  do {
    /* Her turda önce özel köşe durumlarından birini denetler, ardından... */
    if ((row == nrow) && (col == 0))
      corner1(chr++);
    if ((row == nrow - 2) && (col == 0) && (ncol % 4))
      corner2(chr++);
    if ((row == nrow - 2) && (col == 0) && (ncol % 8 == 4))
      corner3(chr++);
    if ((row == nrow + 4) && (col == 2) && (!(ncol % 8)))
      corner4(chr++);
    /* Sıradaki karakterleri ekleyerek çapraz biçimde yukarı tarar... */
    do {
      if ((row < nrow) && (col >= 0) && (!array[row * ncol + col]))
        utah(row, col, chr++);
      row -= 2;
      col += 2;
    } while ((row >= 0) && (col < ncol));
    row += 1;
    col += 3;
    /* Ardından sıradaki karakterleri ekleyerek çapraz biçimde aşağı tarar... */
    do {
      if ((row >= 0) && (col < ncol) && (!array[row * ncol + col]))
        utah(row, col, chr++);
      row += 2;
      col -= 2;
    } while ((row < nrow) && (col >= 0));
    row += 3;
    col += 1;

    /* ...array'in tamamı taranana kadar sürdürür. */
  } while ((row < nrow) || (col < ncol));

  /* Son olarak sağ alt köşeye dokunulmadıysa sabit pattern ile doldurur. */
  if (!array[nrow * ncol - 1]) {
    array[nrow * ncol - 1] = array[nrow * ncol - ncol - 2] = 1;
  }
}

/* "main", command line girdilerini doğrular; ardından array'i hesaplayıp gösterir. */
int main(int argc, char *argv[]) {
  int x, y, z;

  if (argc < 3) {
    printf("Kullanım: ECC200 veri_satırı_sayısı veri_sütunu_sayısı\n");
  } else {
    nrow = ncol = 0;
    nrow = atoi(argv[1]);
    ncol = atoi(argv[2]);
    if ((nrow >= 6) && (~nrow & 0x01) && (ncol >= 6) && (~ncol & 0x01)) {
      array = malloc(sizeof(int) * nrow * ncol);

      ECC200();
      printf("\n");

      for (x = 0; x < nrow; x++) {
        for (y = 0; y < ncol; y++) {
          z = array[x * ncol + y];
          printf("(%d,%d), ", z / 10, z % 10);
        }
        printf("\n");
      }
      free(array);
    }
  }
}
