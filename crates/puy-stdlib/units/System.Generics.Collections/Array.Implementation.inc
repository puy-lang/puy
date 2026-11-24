// ===== TArray (10.4) =====

class function TArray.ByDefaultOrder<T>(const Comparer: IComparer<T>): Boolean;
begin
  Result := Comparer = nil;
  if Result then
    CheckDefaultOrder(Default(T));
end;

class procedure TArray.SortItems<T>(var Items: TArray<T>; const Comparer: IComparer<T>);
const
  // The length of the runs sorted by insertion before merging.
  RunLength = 8;
var
  N, Lo, Mid, Hi, Width, I, J, L, R, C: Integer;
  Src, Dst, Swap: TArray<T>;
  A, B: T;
  ByKey: Boolean;
begin
  // The elements of a type ordered by a built-in comparison are sorted by the VM, the same way.
  if IsPlainOrder(Default(T)) then
    if Comparer = nil then
    begin
      ArraySortRange(Items, 0, Length(Items));
      Exit;
    end;
  // Each comparison below is `KeyCompare<T>(A, B)` when ByKey, else `Comparer.Compare(A, B)`.
  // The loops are written once per kind so that the kind is not tested at every comparison.
  ByKey := ByDefaultOrder<T>(Comparer);
  // The sort works on local arrays (reading a `var` parameter costs one more load per element).
  // Items is cleared so that it never holds an array in the middle of a pass; the callers throw
  // it away when the comparer raises.
  Src := Items;
  Items := nil;
  N := Length(Src);
  Lo := 0;
  while Lo < N do
  begin
    if N - Lo > RunLength then
      Hi := Lo + RunLength
    else
      Hi := N;
    for I := Lo + 1 to Hi - 1 do
    begin
      B := Src[I];
      J := I - 1;
      if ByKey then
        while J >= Lo do
        begin
          A := Src[J];
          if KeyCompare<T>(A, B) <= 0 then
            Break;
          Src[J + 1] := A;
          Dec(J);
        end
      else
        while J >= Lo do
        begin
          A := Src[J];
          if Comparer.Compare(A, B) <= 0 then
            Break;
          Src[J + 1] := A;
          Dec(J);
        end;
      Src[J + 1] := B;
    end;
    Lo := Hi;
  end;
  if N <= RunLength then
  begin
    Items := Src;
    Exit;
  end;
  // Merge pairs of sorted runs from Src into Dst, doubling the width each pass.
  SetLength(Dst, N);
  Width := RunLength;
  while Width < N do
  begin
    Lo := 0;
    while Lo < N do
    begin
      if N - Lo > Width then
        Mid := Lo + Width
      else
        Mid := N;
      if N - Mid > Width then
        Hi := Mid + Width
      else
        Hi := N;
      L := Lo;
      R := Mid;
      I := Lo;
      if Mid < Hi then
      begin
        // Already in order (one comparison, so sorted input is linear): the runs are copied as
        // they are, the left one first.
        A := Src[Mid - 1];
        B := Src[Mid];
        if ByKey then
          C := KeyCompare<T>(A, B)
        else
          C := Comparer.Compare(A, B);
        if C > 0 then
        begin
          // A and B hold Src[L] and Src[R]; only the side that moved is read again. Ties take
          // the left run first, which keeps the sort stable. The loop ends when one run is used
          // up, and the copies below take the rest of the other.
          A := Src[L];
          if ByKey then
            while True do
            begin
              if KeyCompare<T>(A, B) <= 0 then
              begin
                Dst[I] := A;
                Inc(I);
                Inc(L);
                if L = Mid then
                  Break;
                A := Src[L];
              end
              else
              begin
                Dst[I] := B;
                Inc(I);
                Inc(R);
                if R = Hi then
                  Break;
                B := Src[R];
              end;
            end
          else
            while True do
            begin
              if Comparer.Compare(A, B) <= 0 then
              begin
                Dst[I] := A;
                Inc(I);
                Inc(L);
                if L = Mid then
                  Break;
                A := Src[L];
              end
              else
              begin
                Dst[I] := B;
                Inc(I);
                Inc(R);
                if R = Hi then
                  Break;
                B := Src[R];
              end;
            end;
        end;
      end;
      while L < Mid do
      begin
        Dst[I] := Src[L];
        Inc(L);
        Inc(I);
      end;
      while R < Hi do
      begin
        Dst[I] := Src[R];
        Inc(R);
        Inc(I);
      end;
      Lo := Hi;
    end;
    Swap := Src;
    Src := Dst;
    Dst := Swap;
    if Width > N div 2 then
      Width := N
    else
      Width := Width * 2;
  end;
  Items := Src;
end;

class function TArray.SearchItems<T>(const Values: array of T; Count: Integer; const Item: T;
  out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean;
var
  Lo, Hi, Mid, C: Integer;
  ByKey: Boolean;
begin
  // Each comparison below is `KeyCompare<T>(A, B)` when ByKey, else `Comparer.Compare(A, B)`.
  ByKey := ByDefaultOrder<T>(Comparer);
  Lo := 0;
  Hi := Count;
  while Lo < Hi do
  begin
    Mid := Lo + (Hi - Lo) div 2;
    if ByKey then
      C := KeyCompare<T>(Values[Mid], Item)
    else
      C := Comparer.Compare(Values[Mid], Item);
    if C < 0 then
      Lo := Mid + 1
    else
      Hi := Mid;
  end;
  FoundIndex := Lo;
  Result := False;
  if Lo < Count then
  begin
    if ByKey then
      C := KeyCompare<T>(Values[Lo], Item)
    else
      C := Comparer.Compare(Values[Lo], Item);
    Result := C = 0;
  end;
end;

class procedure TArray.Sort<T>(var Values: array of T);
begin
  Sort<T>(Values, nil, 0, Length(Values));
end;

class procedure TArray.Sort<T>(var Values: array of T; const Comparer: IComparer<T>);
begin
  Sort<T>(Values, Comparer, 0, Length(Values));
end;

// Sorts a copy of the range and writes it back once sorted, so that a comparer that raises leaves
// Values as it was (10.3).
class procedure TArray.Sort<T>(var Values: array of T; const Comparer: IComparer<T>;
  Index, Count: Integer);
var
  Work: TArray<T>;
  I: Integer;
begin
  // The same checks and messages as the range of ArrayMoveRange (bytecode.md 2.22).
  if Count < 0 then
    raise ERangeError.Create('array count ' + IntToStr(Count) + ' is out of range');
  if (Index < 0) or (Index > Length(Values) - Count) then
    raise ERangeError.Create('array range ' + IntToStr(Index) + '..' +
      IntToStr(Int64(Index) + Count - 1) + ' is out of range 0..' + IntToStr(High(Values)));
  SetLength(Work, Count);
  for I := 0 to Count - 1 do
    Work[I] := Values[Index + I];
  SortItems<T>(Work, Comparer);
  for I := 0 to Count - 1 do
    Values[Index + I] := Work[I];
end;

class function TArray.BinarySearch<T>(const Values: array of T; const Item: T;
  out FoundIndex: Integer): Boolean;
begin
  Result := SearchItems<T>(Values, Length(Values), Item, FoundIndex, nil);
end;

class function TArray.BinarySearch<T>(const Values: array of T; const Item: T;
  out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean;
begin
  Result := SearchItems<T>(Values, Length(Values), Item, FoundIndex, Comparer);
end;
