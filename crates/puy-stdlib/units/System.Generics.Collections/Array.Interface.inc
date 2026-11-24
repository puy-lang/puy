  // ===== TArray (10.4) =====
type
  // Sorting and binary search of arrays (10.3, 10.4). Only class methods; never instantiated.
  TArray = class
  private
    // Whether to compare with the default order of T (KeyCompare): when Comparer is nil, which
    // raises EInvalidOperation at once when T has none, even before any two values are compared.
    // The default order is compared directly, not through a comparer object.
    class function ByDefaultOrder<T>(const Comparer: IComparer<T>): Boolean; static;
    // Sorts Items stably (a bottom-up merge sort over runs sorted by insertion). Every slot is
    // written once per pass from a slot of the previous pass, so a comparer that breaks its
    // contract cannot lose or repeat an item; Items is left in an unspecified state when the
    // comparer raises, and the callers then throw it away.
    class procedure SortItems<T>(var Items: TArray<T>; const Comparer: IComparer<T>); static;
    // The binary search of Values[0 .. Count - 1] (10.3): the smallest index of an item equal to
    // Item, else the index Item would be inserted at.
    class function SearchItems<T>(const Values: array of T; Count: Integer; const Item: T;
      out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean; static;
  public
    class procedure Sort<T>(var Values: array of T); overload; static;
    class procedure Sort<T>(var Values: array of T; const Comparer: IComparer<T>); overload;
      static;
    class procedure Sort<T>(var Values: array of T; const Comparer: IComparer<T>;
      Index, Count: Integer); overload; static;
    class function BinarySearch<T>(const Values: array of T; const Item: T;
      out FoundIndex: Integer): Boolean; overload; static;
    class function BinarySearch<T>(const Values: array of T; const Item: T;
      out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean; overload; static;
  end;
