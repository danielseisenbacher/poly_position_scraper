# kotlinx.serialization ships its own consumer rules; keep the generated serializers
# of our models explicitly so R8 never strips a field the JSON still carries.
-keepclassmembers @kotlinx.serialization.Serializable class io.github.danielseisenbacher.polywatch.data.** {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class io.github.danielseisenbacher.polywatch.data.**$$serializer { *; }
