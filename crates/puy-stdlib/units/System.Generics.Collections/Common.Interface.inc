  // ===== Common (chapter 4) =====
type
  // The common parent of the containers (4.1): another container can be passed where a
  // TEnumerable<T> is taken. Iterating a value whose static type is TEnumerable<T> walks a copy
  // made by ToArray; the containers declare their own GetEnumerator, which does not copy.
  TEnumerable<T> = class abstract
  public
    type
      // Walks an array made by ToArray.
      TArrayEnumerator = record
      private
        FItems: TArray<T>;
        FIndex: Integer;
        function GetCurrent: T;
      public
        function MoveNext: Boolean;
        property Current: T read GetCurrent;
      end;
    // The items in enumeration order, as a new array.
    function ToArray: TArray<T>; virtual; abstract;
    function GetEnumerator: TArrayEnumerator;
  end;

  // What a container tells its Notify (KeyNotify, ValueNotify) about an item (9.4, 12.1):
  // cnAdding before the item goes in (raising refuses it), cnRemoved after it has left.
  TCollectionNotification = (cnAdding, cnRemoved);

  // An entry of a dictionary (4.2).
  TPair<K, V> = record
    Key: K;
    Value: V;
    constructor Create(const AKey: K; const AValue: V);
  end;
